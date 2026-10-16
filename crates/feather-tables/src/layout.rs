//! Layout tabular independente de Iced: o consumidor fornece a medição real
//! do texto. O armazenamento continua esparso e linhas ocultas não são apagadas.
use std::collections::BTreeSet;

use crate::{EstiloCelula, Tabela};

/// Evita materializar a grade inteira por acidente: a GUI fornece uma janela.
const MAX_FAIXA: usize = 100_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErroLayout {
    ConfiguracaoInvalida,
    CoordenadaInvalida,
    MedicaoInvalida,
    JanelaExcessiva,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MedidaTexto {
    pub largura: f32,
    pub altura_linha: f32,
}

/// Adaptadores (Iced/PDF/etc.) devem medir usando a fonte realmente utilizada.
/// O motor não conhece fontes instaladas nem arredonda pixels físicos.
pub trait MedidorTexto {
    fn medir(&self, texto: &str, estilo: &EstiloCelula) -> MedidaTexto;
}

impl<F> MedidorTexto for F
where
    F: Fn(&str, &EstiloCelula) -> MedidaTexto,
{
    fn medir(&self, texto: &str, estilo: &EstiloCelula) -> MedidaTexto {
        self(texto, estilo)
    }
}

/// Medidor determinístico útil para testes e fallback; não substitui
/// medição tipográfica do Iced, especialmente com fontes proporcionais.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MedidorMonoespacado {
    pub largura_caractere: f32,
    pub altura_linha: f32,
}

impl MedidorTexto for MedidorMonoespacado {
    fn medir(&self, texto: &str, _estilo: &EstiloCelula) -> MedidaTexto {
        MedidaTexto {
            largura: texto.chars().count() as f32 * self.largura_caractere,
            altura_linha: self.altura_linha,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConfigLayout {
    pub largura_padrao: f32,
    pub altura_padrao: f32,
    pub largura_minima: f32,
    pub altura_minima: f32,
    /// Trava a largura produzida pelo auto-fit para evitar colunas gigantes.
    pub largura_maxima: f32,
}
impl Default for ConfigLayout {
    fn default() -> Self {
        Self {
            largura_padrao: 96.0,
            altura_padrao: 24.0,
            largura_minima: 16.0,
            altura_minima: 16.0,
            largura_maxima: 2048.0,
        }
    }
}
impl ConfigLayout {
    fn validar(self) -> bool {
        [
            self.largura_padrao,
            self.altura_padrao,
            self.largura_minima,
            self.altura_minima,
            self.largura_maxima,
        ]
        .iter()
        .all(|v| v.is_finite() && *v > 0.0)
            && self.largura_padrao >= self.largura_minima
            && self.largura_padrao <= self.largura_maxima
            && self.altura_padrao >= self.altura_minima
            && self.largura_maxima >= self.largura_minima
    }
}

/// Uma faixa em coordenadas lógicas do viewport, já descontadas as ocultas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FaixaVisivel {
    pub indice: usize,
    pub origem: f32,
    pub tamanho: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MedidaCelula {
    /// Fragmentos visuais; o valor original em `Tabela` não é modificado.
    pub linhas: Vec<String>,
    pub largura: f32,
    pub altura: f32,
}

/// Fatia de linhas do documento que cabe em uma página ou bloco de altura
/// conhecida. O intervalo é [inicio, fim), inclusive se houver linhas ocultas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PaginaTabela {
    pub inicio: usize,
    pub fim: usize,
    pub altura: f32,
}

/// Ocultações e medição pertencem ao estado de visualização, fora da
/// serialização de `Tabela` e do armazenamento colunar do FeatherSheet.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutTabela {
    config: ConfigLayout,
    linhas_ocultas: BTreeSet<usize>,
    colunas_ocultas: BTreeSet<usize>,
}

impl LayoutTabela {
    pub fn configuracao(&self) -> ConfigLayout {
        self.config
    }

    pub fn novo(config: ConfigLayout) -> Result<Self, ErroLayout> {
        if !config.validar() {
            return Err(ErroLayout::ConfiguracaoInvalida);
        }
        Ok(Self {
            config,
            linhas_ocultas: BTreeSet::new(),
            colunas_ocultas: BTreeSet::new(),
        })
    }

    pub fn linha_oculta(&self, linha: usize) -> bool {
        self.linhas_ocultas.contains(&linha)
    }
    pub fn coluna_oculta(&self, coluna: usize) -> bool {
        self.colunas_ocultas.contains(&coluna)
    }

    pub fn ocultar_linha(
        &mut self,
        tabela: &Tabela,
        linha: usize,
        ocultar: bool,
    ) -> Result<(), ErroLayout> {
        if linha >= tabela.dimensoes().0 {
            return Err(ErroLayout::CoordenadaInvalida);
        }
        if ocultar {
            self.linhas_ocultas.insert(linha);
        } else {
            self.linhas_ocultas.remove(&linha);
        }
        Ok(())
    }
    pub fn ocultar_coluna(
        &mut self,
        tabela: &Tabela,
        coluna: usize,
        ocultar: bool,
    ) -> Result<(), ErroLayout> {
        if coluna >= tabela.dimensoes().1 {
            return Err(ErroLayout::CoordenadaInvalida);
        }
        if ocultar {
            self.colunas_ocultas.insert(coluna);
        } else {
            self.colunas_ocultas.remove(&coluna);
        }
        Ok(())
    }

    pub fn largura(&self, tabela: &Tabela, coluna: usize) -> Result<f32, ErroLayout> {
        if coluna >= tabela.dimensoes().1 {
            return Err(ErroLayout::CoordenadaInvalida);
        }
        Ok(if self.coluna_oculta(coluna) {
            0.0
        } else {
            tabela.largura(coluna).unwrap_or(self.config.largura_padrao)
        })
    }

    pub fn altura(&self, tabela: &Tabela, linha: usize) -> Result<f32, ErroLayout> {
        if linha >= tabela.dimensoes().0 {
            return Err(ErroLayout::CoordenadaInvalida);
        }
        Ok(if self.linha_oculta(linha) {
            0.0
        } else {
            tabela.altura(linha).unwrap_or(self.config.altura_padrao)
        })
    }

    /// As faixas são geradas somente no intervalo visível, em O(fim-inicio),
    /// inclusive para tabelas virtuais com milhões de linhas/colunas.
    fn faixas(
        &self,
        tabela: &Tabela,
        inicio: usize,
        fim: usize,
        linhas: bool,
    ) -> Result<Vec<FaixaVisivel>, ErroLayout> {
        let limite = if linhas { tabela.dimensoes().0 } else { tabela.dimensoes().1 };
        if inicio > fim || fim > limite {
            return Err(ErroLayout::CoordenadaInvalida);
        }
        if fim - inicio > MAX_FAIXA {
            return Err(ErroLayout::JanelaExcessiva);
        }
        let mut resultado = Vec::new();
        let mut cursor = 0.0_f32;
        for indice in inicio..fim {
            let tamanho = if linhas {
                self.altura(tabela, indice)?
            } else {
                self.largura(tabela, indice)?
            };
            if !tamanho.is_finite() || tamanho < 0.0 {
                return Err(ErroLayout::ConfiguracaoInvalida);
            }
            if tamanho == 0.0 {
                continue;
            }
            let proxima = cursor + tamanho;
            if !proxima.is_finite() || proxima <= cursor {
                return Err(ErroLayout::ConfiguracaoInvalida);
            }
            resultado.push(FaixaVisivel {
                indice,
                origem: cursor,
                tamanho,
            });
            cursor = proxima;
        }
        Ok(resultado)
    }
    pub fn linhas_visiveis(
        &self,
        tabela: &Tabela,
        inicio: usize,
        fim: usize,
    ) -> Result<Vec<FaixaVisivel>, ErroLayout> {
        self.faixas(tabela, inicio, fim, true)
    }
    pub fn colunas_visiveis(
        &self,
        tabela: &Tabela,
        inicio: usize,
        fim: usize,
    ) -> Result<Vec<FaixaVisivel>, ErroLayout> {
        self.faixas(tabela, inicio, fim, false)
    }

    /// Quebra a janela de linhas visíveis em páginas, sem dividir uma linha
    /// individual nem examinar células fora da janela. Linhas ocultas não
    /// ocupam altura; uma linha maior que a página fica sozinha para que o
    /// aplicativo decida como dividir seu conteúdo.
    pub fn paginar_linhas(
        &self,
        tabela: &Tabela,
        inicio: usize,
        fim: usize,
        altura_pagina: f32,
    ) -> Result<Vec<PaginaTabela>, ErroLayout> {
        if !altura_pagina.is_finite() || altura_pagina <= 0.0 {
            return Err(ErroLayout::ConfiguracaoInvalida);
        }
        let faixas = self.linhas_visiveis(tabela, inicio, fim)?;
        let mut paginas = Vec::new();
        let mut primeira: Option<usize> = None;
        let mut ultima = inicio;
        let mut usada = 0.0_f32;
        for faixa in faixas {
            let nova = usada + faixa.tamanho;
            if !nova.is_finite() {
                return Err(ErroLayout::ConfiguracaoInvalida);
            }
            if primeira.is_some() && nova > altura_pagina {
                paginas.push(PaginaTabela {
                    inicio: primeira.expect("a pagina iniciada possui indice"),
                    fim: ultima,
                    altura: usada,
                });
                primeira = None;
                usada = 0.0;
            }
            if primeira.is_none() {
                primeira = Some(faixa.indice);
            }
            usada += faixa.tamanho;
            ultima = faixa.indice + 1;
        }
        if let Some(inicio) = primeira {
            paginas.push(PaginaTabela {
                inicio,
                fim: ultima,
                altura: usada,
            });
        }
        Ok(paginas)
    }

    fn medir<M: MedidorTexto + ?Sized>(
        medidor: &M,
        texto: &str,
        estilo: &EstiloCelula,
    ) -> Result<MedidaTexto, ErroLayout> {
        let resultado = medidor.medir(texto, estilo);
        if !resultado.largura.is_finite()
            || resultado.largura < 0.0
            || !resultado.altura_linha.is_finite()
            || resultado.altura_linha <= 0.0
        {
            return Err(ErroLayout::MedicaoInvalida);
        }
        Ok(resultado)
    }

    /// Quebra por parágrafos e palavras; palavras maiores que a coluna são
    /// quebradas apenas em limites UTF-8 de caractere, nunca em bytes soltos.
    fn linhas_texto<M: MedidorTexto + ?Sized>(
        texto: &str,
        estilo: &EstiloCelula,
        largura: f32,
        medidor: &M,
    ) -> Result<Vec<String>, ErroLayout> {
        let mut linhas = Vec::new();
        for paragrafo in texto.split('\n') {
            if !estilo.quebrar_linha || paragrafo.is_empty() {
                linhas.push(paragrafo.to_owned());
                continue;
            }
            let mut inicio = 0;
            while inicio < paragrafo.len() {
                let restante = &paragrafo[inicio..];
                if Self::medir(medidor, restante, estilo)?.largura <= largura {
                    linhas.push(restante.to_owned());
                    break;
                }
                let mut offsets: Vec<usize> = restante.char_indices().map(|(i, _)| i).collect();
                offsets.push(restante.len());
                // Maior prefixo que cabe: busca binária evita medir N prefixos.
                let (mut menor, mut maior) = (1, offsets.len() - 1);
                while menor < maior {
                    let meio = menor + (maior - menor + 1) / 2;
                    if Self::medir(medidor, &restante[..offsets[meio]], estilo)?.largura
                        <= largura
                    {
                        menor = meio;
                    } else {
                        maior = meio - 1;
                    }
                }
                let fim = offsets[menor];
                let prefixo = &restante[..fim];
                // Prefere quebra após palavras quando possível. Espaços no
                // começo da linha seguinte não aparecem, mas a fonte fica intacta.
                if let Some(i) = prefixo.rfind(char::is_whitespace).filter(|i| *i > 0) {
                    linhas.push(prefixo[..i].to_owned());
                    let consumido = fim - (prefixo.len() - i);
                    inicio += consumido;
                    inicio += paragrafo[inicio..]
                        .len()
                        - paragrafo[inicio..].trim_start_matches(char::is_whitespace).len();
                } else {
                    linhas.push(prefixo.to_owned());
                    inicio += fim;
                }
            }
        }
        Ok(linhas)
    }

    fn medidas<M: MedidorTexto + ?Sized>(
        &self,
        tabela: &Tabela,
        linha: usize,
        coluna: usize,
        medidor: &M,
        quebra: bool,
    ) -> Result<MedidaCelula, ErroLayout> {
        if linha >= tabela.dimensoes().0 || coluna >= tabela.dimensoes().1 {
            return Err(ErroLayout::CoordenadaInvalida);
        }
        let completo = tabela
            .estilo_completo(linha, coluna)
            .map_err(|_| ErroLayout::CoordenadaInvalida)?;
        if !completo.layout.valido() {
            return Err(ErroLayout::ConfiguracaoInvalida);
        }
        let estilo = &completo.basico;
        let layout = &completo.layout;
        let padding = &layout.padding;
        let largura_disponivel = if quebra {
            let mut largura = self.largura(tabela, coluna)?;
            // Célula âncora pode ocupar mais colunas; não força layout denso.
            if let Some(m) = tabela.mesclagens().iter().find(|m| m.linha == linha && m.coluna == coluna) {
                largura = 0.0;
                for c in m.coluna..m.coluna + m.colunas {
                    largura += self.largura(tabela, c)?;
                }
            }
            (largura - padding.esquerda - padding.direita).max(0.0)
        } else {
            f32::INFINITY
        };
        let valor = tabela.obter(linha, coluna).map(|c| c.valor.texto()).unwrap_or_default();
        // Normaliza somente a visualização, nunca o valor armazenado.
        let normalizado = valor.replace("\r\n", "\n").replace('\r', "\n");
        let linhas = Self::linhas_texto(&normalizado, estilo, largura_disponivel, medidor)?;
        let mut largura = 0.0_f32;
        let mut altura = 0.0_f32;
        for linha in &linhas {
            let tamanho = Self::medir(medidor, linha, estilo)?;
            largura = largura.max(tamanho.largura);
            altura += tamanho.altura_linha * layout.entre_linhas;
        }
        let paragrafos = normalizado.matches('\n').count();
        altura += (paragrafos as f32) * layout.entre_paragrafos;
        largura += padding.esquerda + padding.direita;
        altura += padding.cima + padding.baixo;
        if !largura.is_finite() || !altura.is_finite() {
            return Err(ErroLayout::MedicaoInvalida);
        }
        Ok(MedidaCelula { linhas, largura, altura })
    }

    /// Mede texto renderizável incluindo padding, entrelinha e múltiplos
    /// parágrafos. `quebrar_linha` é respeitado sem alterar a célula.
    pub fn medir_celula<M: MedidorTexto + ?Sized>(
        &self,
        tabela: &Tabela,
        linha: usize,
        coluna: usize,
        medidor: &M,
    ) -> Result<MedidaCelula, ErroLayout> {
        self.medidas(tabela, linha, coluna, medidor, true)
    }

    /// Usa somente células existentes na BTreeMap; valores de células
    /// mescladas que abrangem várias colunas não esticam uma única coluna.
    pub fn largura_auto<M: MedidorTexto + ?Sized>(
        &self,
        tabela: &Tabela,
        coluna: usize,
        medidor: &M,
    ) -> Result<f32, ErroLayout> {
        if coluna >= tabela.dimensoes().1 {
            return Err(ErroLayout::CoordenadaInvalida);
        }
        let mut largura = self.config.largura_minima;
        // Inclui padding de células vazias que possuem apenas layout.
        let posicoes = tabela.iter().map(|(pos, _)| *pos)
            .chain(tabela.iter_layout().map(|(pos, _)| *pos));
        for (linha, c) in posicoes {
            if c != coluna {
                continue;
            }
            if self.linha_oculta(linha) {
                continue;
            }
            if tabela.mesclagens().iter().any(|m| {
                m.linha == linha && m.coluna == c && m.colunas > 1
            }) {
                continue;
            }
            largura = largura.max(self.medidas(tabela, linha, c, medidor, false)?.largura);
        }
        Ok(largura.clamp(self.config.largura_minima, self.config.largura_maxima))
    }

    /// Mede o texto com largura efetiva de cada coluna, incluindo quebras.
    /// Regiões que abrangem diversas linhas não aumentam uma linha isolada.
    pub fn altura_auto<M: MedidorTexto + ?Sized>(
        &self,
        tabela: &Tabela,
        linha: usize,
        medidor: &M,
    ) -> Result<f32, ErroLayout> {
        if linha >= tabela.dimensoes().0 {
            return Err(ErroLayout::CoordenadaInvalida);
        }
        let mut altura = self.config.altura_minima;
        let posicoes = tabela.iter().map(|(pos, _)| *pos)
            .chain(tabela.iter_layout().map(|(pos, _)| *pos));
        for (r, coluna) in posicoes {
            if r != linha {
                continue;
            }
            if self.coluna_oculta(coluna) {
                continue;
            }
            if tabela.mesclagens().iter().any(|m| {
                m.linha == r && m.coluna == coluna && m.linhas > 1
            }) {
                continue;
            }
            altura = altura.max(self.medir_celula(tabela, r, coluna, medidor)?.altura);
        }
        Ok(altura)
    }

    pub fn auto_ajustar_coluna<M: MedidorTexto + ?Sized>(
        &self,
        tabela: &mut Tabela,
        coluna: usize,
        medidor: &M,
    ) -> Result<f32, ErroLayout> {
        let largura = self.largura_auto(tabela, coluna, medidor)?;
        tabela.definir_largura(coluna, largura)
            .map_err(|_| ErroLayout::CoordenadaInvalida)?;
        Ok(largura)
    }
    pub fn auto_ajustar_linha<M: MedidorTexto + ?Sized>(
        &self,
        tabela: &mut Tabela,
        linha: usize,
        medidor: &M,
    ) -> Result<f32, ErroLayout> {
        let altura = self.altura_auto(tabela, linha, medidor)?;
        tabela.definir_altura(linha, altura)
            .map_err(|_| ErroLayout::CoordenadaInvalida)?;
        Ok(altura)
    }
}

impl Default for LayoutTabela {
    fn default() -> Self {
        Self::novo(ConfigLayout::default()).expect("configuracao padrao valida")
    }
}
