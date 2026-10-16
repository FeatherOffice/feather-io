//! Seleção e transferência tabular sem depender de Iced, SO ou clipboard.
//! A GUI mapeia teclas/mouse para estas operações; o FeatherSheet mantém seu
//! motor de fórmulas e seu armazenamento colunar independentes.

use std::collections::BTreeMap;

use crate::{Celula, EstiloLayout, Regiao, Tabela, Valor};

const MAX_CELULAS: usize = 100_000;
const MAX_TSV_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErroSelecao {
    ForaDosLimites,
    AreaExcessiva,
    MesclagemInterrompida,
    TextoInvalido,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PosicaoCelula {
    pub linha: usize,
    pub coluna: usize,
}

impl PosicaoCelula {
    pub const fn nova(linha: usize, coluna: usize) -> Self {
        Self { linha, coluna }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Navegacao {
    Cima,
    Baixo,
    Esquerda,
    Direita,
    Tab,
    ShiftTab,
    InicioLinha,
    FimLinha,
    InicioTabela,
    FimTabela,
}

/// Âncora fixa enquanto Shift está pressionado; ativo acompanha mouse/tecla.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelecaoTabela {
    pub ancora: PosicaoCelula,
    pub ativo: PosicaoCelula,
}

impl SelecaoTabela {
    pub fn nova(tabela: &Tabela, posicao: PosicaoCelula) -> Result<Self, ErroSelecao> {
        let posicao = normalizar(tabela, posicao)?;
        Ok(Self { ancora: posicao, ativo: posicao })
    }

    pub fn selecionar_ate(
        &mut self,
        tabela: &Tabela,
        posicao: PosicaoCelula,
    ) -> Result<(), ErroSelecao> {
        self.ativo = normalizar(tabela, posicao)?;
        Ok(())
    }

    pub fn regiao(self) -> Regiao {
        let li = self.ancora.linha.min(self.ativo.linha);
        let co = self.ancora.coluna.min(self.ativo.coluna);
        Regiao {
            linha: li,
            coluna: co,
            linhas: self.ancora.linha.max(self.ativo.linha) - li + 1,
            colunas: self.ancora.coluna.max(self.ativo.coluna) - co + 1,
        }
    }

    /// Move a célula ativa. Quando expandir=false, a âncora acompanha o cursor.
    /// Interior de mesclagem é normalizado à âncora; Direita/Baixo salta
    /// a largura/altura da região mesclada, quando há espaço.
    pub fn mover(
        &mut self,
        tabela: &Tabela,
        direcao: Navegacao,
        expandir: bool,
    ) -> Result<(), ErroSelecao> {
        let (linhas, colunas) = tabela.dimensoes();
        let atual = normalizar(tabela, self.ativo)?;
        let mescla = tabela.mesclagens().iter().find(|m| {
            m.linha == atual.linha && m.coluna == atual.coluna
        });
        let nova = match direcao {
            Navegacao::Cima => PosicaoCelula::nova(atual.linha.saturating_sub(1), atual.coluna),
            Navegacao::Baixo => PosicaoCelula::nova(
                mescla.map_or(atual.linha.saturating_add(1), |m| m.linha + m.linhas)
                    .min(linhas - 1),
                atual.coluna,
            ),
            Navegacao::Esquerda => {
                PosicaoCelula::nova(atual.linha, atual.coluna.saturating_sub(1))
            }
            Navegacao::Direita => PosicaoCelula::nova(
                atual.linha,
                mescla.map_or(atual.coluna.saturating_add(1), |m| m.coluna + m.colunas)
                    .min(colunas - 1),
            ),
            Navegacao::InicioLinha => PosicaoCelula::nova(atual.linha, 0),
            Navegacao::FimLinha => PosicaoCelula::nova(atual.linha, colunas - 1),
            Navegacao::InicioTabela => PosicaoCelula::nova(0, 0),
            Navegacao::FimTabela => PosicaoCelula::nova(linhas - 1, colunas - 1),
            Navegacao::Tab => {
                let proxima = mescla.map_or(atual.coluna.saturating_add(1), |m| m.coluna + m.colunas);
                if proxima < colunas {
                    PosicaoCelula::nova(atual.linha, proxima)
                } else if atual.linha + 1 < linhas {
                    PosicaoCelula::nova(atual.linha + 1, 0)
                } else {
                    PosicaoCelula::nova(atual.linha, colunas - 1)
                }
            }
            Navegacao::ShiftTab => {
                if atual.coluna > 0 {
                    PosicaoCelula::nova(atual.linha, atual.coluna - 1)
                } else if atual.linha > 0 {
                    PosicaoCelula::nova(atual.linha - 1, colunas - 1)
                } else {
                    atual
                }
            }
        };
        let novo_ativo = normalizar(tabela, nova)?;
        self.ativo = novo_ativo;
        if !expandir {
            self.ancora = novo_ativo;
        }
        Ok(())
    }
}

fn normalizar(tabela: &Tabela, p: PosicaoCelula) -> Result<PosicaoCelula, ErroSelecao> {
    let (linhas, colunas) = tabela.dimensoes();
    if p.linha >= linhas || p.coluna >= colunas {
        return Err(ErroSelecao::ForaDosLimites);
    }
    if let Some(m) = tabela.mesclagens().iter().find(|m| m.contem_celula(p.linha, p.coluna)) {
        return Ok(PosicaoCelula::nova(m.linha, m.coluna));
    }
    Ok(p)
}

fn validar_area(tabela: &Tabela, regiao: Regiao) -> Result<(), ErroSelecao> {
    let (linhas, colunas) = tabela.dimensoes();
    if regiao.linhas == 0
        || regiao.colunas == 0
        || regiao.linha.checked_add(regiao.linhas).map_or(true, |x| x > linhas)
        || regiao.coluna.checked_add(regiao.colunas).map_or(true, |x| x > colunas)
    {
        return Err(ErroSelecao::ForaDosLimites);
    }
    if regiao.linhas.checked_mul(regiao.colunas).map_or(true, |n| n > MAX_CELULAS) {
        return Err(ErroSelecao::AreaExcessiva);
    }
    // A transferência simples não tenta reconstruir vínculos de merge
    // (TBL-010): recuse inclusive seleção parcial para evitar perda de dados.
    if tabela.mesclagens().iter().any(|m| m.intersecta(regiao)) {
        return Err(ErroSelecao::MesclagemInterrompida);
    }
    Ok(())
}

/// Porta implementada pelo consumidor (Iced, navegador, sistema operacional).
/// O motor comum jamais acessa diretamente o clipboard do sistema.
pub trait AdaptadorTransferencia {
    type Erro;
    fn obter_texto(&mut self) -> Result<String, Self::Erro>;
    fn definir_texto(&mut self, texto: &str) -> Result<(), Self::Erro>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErroAdaptador<E> {
    Selecao(ErroSelecao),
    Externo(E),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModoColagem {
    Tudo,
    SomenteValores,
    SomenteEstilos,
}

#[derive(Debug, Clone, PartialEq, Default)]
struct EntradaCopiada {
    celula: Celula,
    layout: EstiloLayout,
}

/// Conteúdo rico e esparso: copia valores tipados + estilo básico/complementar,
/// inclusive layouts em células sem valor. Não inclui fórmulas do Sheet nem
/// dependências de APIs de clipboard de sistemas operacionais.
#[derive(Debug, Clone, PartialEq)]
pub struct AreaCopiada {
    linhas: usize,
    colunas: usize,
    celulas: BTreeMap<(usize, usize), EntradaCopiada>,
}

impl AreaCopiada {
    pub fn dimensoes(&self) -> (usize, usize) {
        (self.linhas, self.colunas)
    }

    pub fn entradas_armazenadas(&self) -> usize {
        self.celulas.len()
    }

    pub fn copiar(tabela: &Tabela, regiao: Regiao) -> Result<Self, ErroSelecao> {
        validar_area(tabela, regiao)?;
        let mut celulas = BTreeMap::<(usize, usize), EntradaCopiada>::new();
        for (&(linha, coluna), celula) in tabela.iter() {
            if regiao.contem_celula(linha, coluna) {
                let chave = (linha - regiao.linha, coluna - regiao.coluna);
                celulas.entry(chave).or_default().celula = celula.clone();
            }
        }
        for (&(linha, coluna), layout) in tabela.iter_layout() {
            if regiao.contem_celula(linha, coluna) {
                let chave = (linha - regiao.linha, coluna - regiao.coluna);
                celulas.entry(chave).or_default().layout = layout.clone();
            }
        }
        Ok(Self { linhas: regiao.linhas, colunas: regiao.colunas, celulas })
    }

    /// Colagem pré-validada: dimensões, mesclagens e layouts são conferidos
    /// ANTES de qualquer escrita. Substitui células vazias corretamente.
    pub fn colar(
        &self,
        tabela: &mut Tabela,
        destino: PosicaoCelula,
        modo: ModoColagem,
    ) -> Result<Regiao, ErroSelecao> {
        let regiao = Regiao {
            linha: destino.linha,
            coluna: destino.coluna,
            linhas: self.linhas,
            colunas: self.colunas,
        };
        validar_area(tabela, regiao)?;
        if self.celulas.values().any(|entrada| !entrada.layout.valido()) {
            return Err(ErroSelecao::TextoInvalido);
        }
        for dr in 0..self.linhas {
            for dc in 0..self.colunas {
                let linha = destino.linha + dr;
                let coluna = destino.coluna + dc;
                let dados = self.celulas.get(&(dr, dc));
                match modo {
                    ModoColagem::Tudo => {
                        let celula = dados.map(|e| e.celula.clone()).unwrap_or_default();
                        let layout = dados.map(|e| e.layout.clone()).unwrap_or_default();
                        tabela.definir(linha, coluna, celula)
                            .expect("posição previamente validada");
                        tabela.definir_layout(linha, coluna, layout)
                            .expect("layout previamente validado");
                    }
                    ModoColagem::SomenteValores => {
                        let valor = dados.map(|e| e.celula.valor.clone()).unwrap_or_default();
                        tabela.atualizar_valor(linha, coluna, valor)
                            .expect("posição previamente validada");
                    }
                    ModoColagem::SomenteEstilos => {
                        let basico = dados.map(|e| e.celula.estilo.clone()).unwrap_or_default();
                        let layout = dados.map(|e| e.layout.clone()).unwrap_or_default();
                        tabela.atualizar_estilo(linha, coluna, basico)
                            .expect("posição previamente validada");
                        tabela.definir_layout(linha, coluna, layout)
                            .expect("layout previamente validado");
                    }
                }
            }
        }
        Ok(regiao)
    }

    /// Publica uma cópia textual sem acessar APIs de UI ou sistema operacional.
    pub fn copiar_para<A: AdaptadorTransferencia>(
        &self,
        adaptador: &mut A,
    ) -> Result<(), ErroAdaptador<A::Erro>> {
        let texto = self.para_tsv().map_err(ErroAdaptador::Selecao)?;
        adaptador.definir_texto(&texto).map_err(ErroAdaptador::Externo)
    }

    /// Recebe texto externo como valores, sem inferir fórmulas, tipos ou estilos.
    pub fn receber_de<A: AdaptadorTransferencia>(
        adaptador: &mut A,
    ) -> Result<Self, ErroAdaptador<A::Erro>> {
        let texto = adaptador.obter_texto().map_err(ErroAdaptador::Externo)?;
        Self::de_tsv(&texto).map_err(ErroAdaptador::Selecao)
    }

    /// Representação interoperável com clipboard texto (tabulações e
    /// quebras). Não preserva tipos/estilos; use AreaCopiada para copiar rico.
    pub fn para_tsv(&self) -> Result<String, ErroSelecao> {
        if self.linhas.checked_mul(self.colunas).map_or(true, |n| n > MAX_CELULAS) {
            return Err(ErroSelecao::AreaExcessiva);
        }
        let mut saida = String::new();
        for linha in 0..self.linhas {
            if linha > 0 {
                saida.push('\n');
            }
            for coluna in 0..self.colunas {
                if coluna > 0 {
                    saida.push('\t');
                }
                let valor = self.celulas.get(&(linha, coluna))
                    .map(|v| v.celula.valor.texto()).unwrap_or_default();
                if valor.chars().any(|c| matches!(c, '\t' | '\r' | '\n' | '"')) {
                    saida.push('"');
                    saida.push_str(&valor.replace('"', "\"\""));
                    saida.push('"');
                } else {
                    saida.push_str(&valor);
                }
                if saida.len() > MAX_TSV_BYTES {
                    return Err(ErroSelecao::AreaExcessiva);
                }
            }
        }
        Ok(saida)
    }

    /// Parser TSV com aspas e delimitadores CRLF, compatível com Excel/Calc.
    /// Texto externo vira Valor::Texto e nunca tenta injetar fórmulas.
    pub fn de_tsv(texto: &str) -> Result<Self, ErroSelecao> {
        if texto.len() > MAX_TSV_BYTES {
            return Err(ErroSelecao::AreaExcessiva);
        }
        let mut linhas = Vec::<Vec<String>>::new();
        let mut campos_totais = 0usize;
        let mut linha = Vec::new();
        let mut campo = String::new();
        let mut chars = texto.chars().peekable();
        let mut aspas = false;
        let mut fechado = false;
        let mut inicio = true;
        let mut terminou_linha = false;

        while let Some(c) = chars.next() {
            if aspas {
                if c == '"' {
                    if chars.peek() == Some(&'"') {
                        chars.next();
                        campo.push('"');
                    } else {
                        aspas = false;
                        fechado = true;
                    }
                } else {
                    campo.push(c);
                }
                terminou_linha = false;
                continue;
            }
            match c {
                '"' if inicio => {
                    aspas = true;
                    inicio = false;
                    terminou_linha = false;
                }
                '\t' | '\r' | '\n' => {
                    if c == '\r' && chars.peek() == Some(&'\n') {
                        chars.next();
                    }
                    linha.push(std::mem::take(&mut campo));
                    campos_totais += 1;
                    if campos_totais > MAX_CELULAS || linha.len() > MAX_CELULAS {
                        return Err(ErroSelecao::AreaExcessiva);
                    }
                    inicio = true;
                    fechado = false;
                    terminou_linha = c != '\t';
                    if c != '\t' {
                        linhas.push(std::mem::take(&mut linha));
                        if linhas.len() > MAX_CELULAS {
                            return Err(ErroSelecao::AreaExcessiva);
                        }
                    }
                }
                '"' => return Err(ErroSelecao::TextoInvalido),
                _ if fechado => return Err(ErroSelecao::TextoInvalido),
                _ => {
                    campo.push(c);
                    inicio = false;
                    terminou_linha = false;
                }
            }
        }
        if aspas {
            return Err(ErroSelecao::TextoInvalido);
        }
        if !terminou_linha {
            linha.push(campo);
            campos_totais += 1;
            if campos_totais > MAX_CELULAS || linha.len() > MAX_CELULAS {
                return Err(ErroSelecao::AreaExcessiva);
            }
            linhas.push(linha);
        }
        let altura = linhas.len();
        let largura = linhas.iter().map(Vec::len).max().unwrap_or(0);
        if altura == 0 || largura == 0
            || altura.checked_mul(largura).map_or(true, |n| n > MAX_CELULAS)
        {
            return Err(ErroSelecao::AreaExcessiva);
        }
        let mut celulas = BTreeMap::new();
        for (i, linha) in linhas.into_iter().enumerate() {
            for (j, texto) in linha.into_iter().enumerate() {
                if !texto.is_empty() {
                    celulas.insert((i, j), EntradaCopiada {
                        celula: Celula {
                            valor: Valor::Texto(texto),
                            ..Default::default()
                        },
                        ..Default::default()
                    });
                }
            }
        }
        Ok(Self { linhas: altura, colunas: largura, celulas })
    }
}
