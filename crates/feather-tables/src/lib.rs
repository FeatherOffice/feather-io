//! Contrato tabular compartilhado por planilhas, documentos e apresentações.
//! Armazena somente células usadas; fórmulas e recálculo ficam no FeatherSheet.

pub mod layout;
pub mod selecao;
pub use selecao::{
    AdaptadorTransferencia, AreaCopiada, ErroAdaptador, ErroSelecao, ModoColagem,
    Navegacao, PosicaoCelula, SelecaoTabela,
};
pub use layout::{
    ConfigLayout, ErroLayout, FaixaVisivel, LayoutTabela, MedidaCelula, MedidaTexto,
    MedidorMonoespacado, MedidorTexto, PaginaTabela,
};

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Default)]
pub enum Valor {
    #[default]
    Vazio,
    Texto(String),
    Numero(f64),
    Logico(bool),
}
impl Valor {
    pub fn texto(&self) -> String {
        match self {
            Self::Vazio => String::new(),
            Self::Texto(s) => s.clone(),
            Self::Numero(n) => n.to_string(),
            Self::Logico(b) => b.to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Traco {
    #[default]
    Nenhum,
    Solido,
    Tracejado,
    Pontilhado,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Borda {
    pub estilo: Traco,
    pub espessura: u16,
    pub cor: [u8; 4],
}
impl Default for Borda {
    fn default() -> Self {
        Self {
            estilo: Traco::Nenhum,
            espessura: 0,
            cor: [0, 0, 0, 255],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct EstiloCelula {
    pub fonte: Option<String>,
    pub tamanho: Option<f32>,
    pub negrito: bool,
    pub italico: bool,
    pub sublinhado: bool,
    pub fundo: Option<[u8; 4]>,
    pub texto: Option<[u8; 4]>,
    pub bordas: [Borda; 4], // cima, direita, baixo, esquerda
    pub quebrar_linha: bool,
}

/**
 * Propriedades complementares de layout. Fonte, cor e bordas legadas
 * permanecem em `EstiloCelula` para compatibilidade com consumidores.
 */
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AlinhamentoHorizontal {
    #[default]
    Inicio,
    Centro,
    Fim,
    Justificado,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AlinhamentoVertical {
    #[default]
    Superior,
    Centro,
    Inferior,
}
/// Recuos em unidades lógicas do documento (o adaptador decide pt/px).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Espacamento {
    pub cima: f32,
    pub direita: f32,
    pub baixo: f32,
    pub esquerda: f32,
}
impl Espacamento {
    pub fn valido(self) -> bool {
        [self.cima, self.direita, self.baixo, self.esquerda]
            .iter()
            .all(|v| v.is_finite() && *v >= 0.0)
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct EstiloLayout {
    pub horizontal: AlinhamentoHorizontal,
    pub vertical: AlinhamentoVertical,
    /// Multiplicador de altura de linha; 1.0 é espaçamento simples.
    pub entre_linhas: f32,
    /// Distância após parágrafo em unidades lógicas do documento.
    pub entre_paragrafos: f32,
    pub padding: Espacamento,
    /// Espessuras opcionais (topo, direita, baixo, esquerda), sobrepondo
    /// somente o u16 legado em `EstiloCelula::bordas`.
    pub larguras_bordas: [Option<f32>; 4],
}
impl Default for EstiloLayout {
    fn default() -> Self {
        Self {
            horizontal: AlinhamentoHorizontal::Inicio,
            vertical: AlinhamentoVertical::Superior,
            entre_linhas: 1.0,
            entre_paragrafos: 0.0,
            padding: Espacamento::default(),
            larguras_bordas: [None; 4],
        }
    }
}
impl EstiloLayout {
    pub fn valido(&self) -> bool {
        self.entre_linhas.is_finite()
            && self.entre_linhas > 0.0
            && self.entre_paragrafos.is_finite()
            && self.entre_paragrafos >= 0.0
            && self.padding.valido()
            && self
                .larguras_bordas
                .iter()
                .flatten()
                .all(|v| v.is_finite() && *v >= 0.0)
    }
}
/// Representação combinada, sem mudar os campos públicos de `Celula`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EstiloCompleto {
    pub basico: EstiloCelula,
    pub layout: EstiloLayout,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PoliticaBordas {
    /// Mantém as bordas de cada célula, sem conflito/resolução.
    #[default]
    Separadas,
    /// Uma borda para cada limite compartilhado (maior espessura vence).
    Colapsadas,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lado {
    Cima,
    Direita,
    Baixo,
    Esquerda,
}
impl Lado {
    fn indice(self) -> usize {
        match self {
            Self::Cima => 0,
            Self::Direita => 1,
            Self::Baixo => 2,
            Self::Esquerda => 3,
        }
    }
    fn oposto(self) -> Self {
        match self {
            Self::Cima => Self::Baixo,
            Self::Direita => Self::Esquerda,
            Self::Baixo => Self::Cima,
            Self::Esquerda => Self::Direita,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BordaVisual {
    pub estilo: Traco,
    pub espessura: f32,
    pub cor: [u8; 4],
}
impl Default for BordaVisual {
    fn default() -> Self {
        Self {
            estilo: Traco::Nenhum,
            espessura: 0.0,
            cor: [0, 0, 0, 255],
        }
    }
}
impl BordaVisual {
    fn prioridade(self) -> (u8, u8) {
        let visivel = self.estilo != Traco::Nenhum && self.espessura > 0.0;
        let ordem = match self.estilo {
            Traco::Nenhum => 0,
            Traco::Pontilhado => 1,
            Traco::Tracejado => 2,
            Traco::Solido => 3,
        };
        (u8::from(visivel), ordem)
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Celula {
    pub valor: Valor,
    pub estilo: EstiloCelula,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Regiao {
    pub linha: usize,
    pub coluna: usize,
    pub linhas: usize,
    pub colunas: usize,
}
impl Regiao {
    pub fn intersecta(self, outra: Self) -> bool {
        let a = self.linha.saturating_add(self.linhas);
        let b = self.coluna.saturating_add(self.colunas);
        let c = outra.linha.saturating_add(outra.linhas);
        let d = outra.coluna.saturating_add(outra.colunas);
        self.linha < c && outra.linha < a && self.coluna < d && outra.coluna < b
    }

    /// Inclui as bordas superiores/esquerdas e exclui as inferiores/direitas.
    pub fn contem_celula(self, linha: usize, coluna: usize) -> bool {
        linha >= self.linha
            && coluna >= self.coluna
            && linha < self.linha.saturating_add(self.linhas)
            && coluna < self.coluna.saturating_add(self.colunas)
    }

    /// A região menor deve estar integralmente contida, não apenas intersectar.
    pub fn contem_regiao(self, outra: Self) -> bool {
        outra.linha >= self.linha
            && outra.coluna >= self.coluna
            && outra.linha.saturating_add(outra.linhas)
                <= self.linha.saturating_add(self.linhas)
            && outra.coluna.saturating_add(outra.colunas)
                <= self.coluna.saturating_add(self.colunas)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErroTabela {
    ForaDosLimites,
    MesclagemInvalida,
    Sobreposicao,
    DadosNaMesclagem,
}

/// Direção da linha de corte de uma região mesclada.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DivisaoMesclagem {
    /// Divide nas linhas: uma região acima e outra abaixo de `em`.
    Horizontal,
    /// Divide nas colunas: uma região à esquerda e outra à direita de `em`.
    Vertical,
}

/// Token reversível de alteração de mesclagens; não clona as células esparsas.
/// O consumidor pode empilhar tokens no histórico da aplicação.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use = "guarde a edicao para permitir desfazer/refazer"]
pub struct EdicaoMesclagem {
    antes: Vec<Regiao>,
    depois: Vec<Regiao>,
}
impl EdicaoMesclagem {
    /// Reverte somente se as mesclagens ainda correspondem ao estado produzido.
    /// Revalida células/layouts atuais para não ocultar dados inseridos depois.
    pub fn desfazer(&self, tabela: &mut Tabela) -> Result<(), ErroTabela> {
        if tabela.mesclagens != self.depois {
            return Err(ErroTabela::MesclagemInvalida);
        }
        tabela.aplicar_mesclagens(self.antes.clone())
    }

    /// Reexecuta uma alteração desfeita; rejeita histórico fora de ordem.
    pub fn refazer(&self, tabela: &mut Tabela) -> Result<(), ErroTabela> {
        if tabela.mesclagens != self.antes {
            return Err(ErroTabela::MesclagemInvalida);
        }
        tabela.aplicar_mesclagens(self.depois.clone())
    }

    pub fn antes(&self) -> &[Regiao] {
        &self.antes
    }

    pub fn depois(&self) -> &[Regiao] {
        &self.depois
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tabela {
    linhas: usize,
    colunas: usize,
    celulas: BTreeMap<(usize, usize), Celula>,
    mesclagens: Vec<Regiao>,
    larguras: BTreeMap<usize, f32>,
    alturas: BTreeMap<usize, f32>,
    estilos_layout: BTreeMap<(usize, usize), EstiloLayout>,
    politica_bordas: PoliticaBordas,
}
impl Tabela {
    pub fn nova(linhas: usize, colunas: usize) -> Self {
        Self {
            linhas,
            colunas,
            celulas: BTreeMap::new(),
            mesclagens: Vec::new(),
            larguras: BTreeMap::new(),
            alturas: BTreeMap::new(),
            estilos_layout: BTreeMap::new(),
            politica_bordas: PoliticaBordas::Separadas,
        }
    }
    pub fn dimensoes(&self) -> (usize, usize) {
        (self.linhas, self.colunas)
    }
    pub fn obter(&self, linha: usize, coluna: usize) -> Option<&Celula> {
        self.celulas.get(&(linha, coluna))
    }
    pub fn iter(&self) -> impl Iterator<Item = (&(usize, usize), &Celula)> {
        self.celulas.iter()
    }
    /// Layouts esparsos também existem em células sem valor/estilo básico.
    /// Permite que medidores considerem padding nessas posições sem grade densa.
    pub fn iter_layout(&self) -> impl Iterator<Item = (&(usize, usize), &EstiloLayout)> {
        self.estilos_layout.iter()
    }
    pub fn mesclagens(&self) -> &[Regiao] {
        &self.mesclagens
    }
    pub fn definir(
        &mut self,
        linha: usize,
        coluna: usize,
        celula: Celula,
    ) -> Result<(), ErroTabela> {
        if linha >= self.linhas || coluna >= self.colunas {
            return Err(ErroTabela::ForaDosLimites);
        }
        if self.mesclagens.iter().any(|m| {
            linha >= m.linha
                && coluna >= m.coluna
                && linha < m.linha + m.linhas
                && coluna < m.coluna + m.colunas
                && (linha, coluna) != (m.linha, m.coluna)
        }) {
            return Err(ErroTabela::Sobreposicao);
        }
        if celula == Celula::default() {
            self.celulas.remove(&(linha, coluna));
        } else {
            self.celulas.insert((linha, coluna), celula);
        }
        Ok(())
    }
    /// Recusa regiões inválidas e células/layouts não vazios que perderiam
    /// sua posição visível. O custo é proporcional às células armazenadas.
    fn validar_regiao_mesclada(&self, r: Regiao) -> Result<(), ErroTabela> {
        if r.linhas == 0
            || r.colunas == 0
            || (r.linhas == 1 && r.colunas == 1)
            || r.linha
                .checked_add(r.linhas)
                .map_or(true, |n| n > self.linhas)
            || r.coluna
                .checked_add(r.colunas)
                .map_or(true, |n| n > self.colunas)
        {
            return Err(ErroTabela::MesclagemInvalida);
        }
        if self
            .celulas
            .keys()
            .chain(self.estilos_layout.keys())
            .any(|&(l, c)| r.contem_celula(l, c) && (l, c) != (r.linha, r.coluna))
        {
            return Err(ErroTabela::DadosNaMesclagem);
        }
        Ok(())
    }

    /// Substituição transacional: nenhuma alteração ocorre se houver
    /// sobreposição, ocupação indevida ou coordenadas inválidas.
    fn aplicar_mesclagens(&mut self, mut regioes: Vec<Regiao>) -> Result<(), ErroTabela> {
        regioes.sort_unstable_by_key(|r| (r.linha, r.coluna, r.linhas, r.colunas));
        for (i, &r) in regioes.iter().enumerate() {
            self.validar_regiao_mesclada(r)?;
            if regioes[..i].iter().any(|p| p.intersecta(r)) {
                return Err(ErroTabela::Sobreposicao);
            }
        }
        self.mesclagens = regioes;
        Ok(())
    }

    /// Mantém as regiões ordenadas e sem sobreposição, sem juntar células
    /// vizinhas (elas podem ter semânticas distintas no documento).
    pub fn normalizar_mesclagens(&mut self) -> Result<(), ErroTabela> {
        self.aplicar_mesclagens(self.mesclagens.clone())
    }

    /// Mesclagem legada, mantida para consumidores existentes.
    pub fn mesclar(&mut self, r: Regiao) -> Result<(), ErroTabela> {
        self.validar_regiao_mesclada(r)?;
        if self.mesclagens.iter().any(|m| m.intersecta(r)) {
            return Err(ErroTabela::Sobreposicao);
        }
        self.mesclagens.push(r);
        self.mesclagens
            .sort_unstable_by_key(|m| (m.linha, m.coluna, m.linhas, m.colunas));
        Ok(())
    }

    /// Variante reversível da mesclagem legada.
    pub fn mesclar_com_undo(&mut self, r: Regiao) -> Result<EdicaoMesclagem, ErroTabela> {
        let antes = self.mesclagens.clone();
        self.mesclar(r)?;
        Ok(EdicaoMesclagem {
            antes,
            depois: self.mesclagens.clone(),
        })
    }

    fn indice_mesclagem(&self, linha: usize, coluna: usize) -> Result<usize, ErroTabela> {
        if linha >= self.linhas || coluna >= self.colunas {
            return Err(ErroTabela::ForaDosLimites);
        }
        self.mesclagens
            .iter()
            .position(|r| r.contem_celula(linha, coluna))
            .ok_or(ErroTabela::MesclagemInvalida)
    }

    /// Desmescla a região que contém a célula, inclusive quando a célula
    /// indicada não é a âncora. Não remove conteúdo, largura, altura ou estilo.
    pub fn desmesclar(&mut self, linha: usize, coluna: usize) -> Result<Regiao, ErroTabela> {
        let indice = self.indice_mesclagem(linha, coluna)?;
        Ok(self.mesclagens.remove(indice))
    }

    pub fn desmesclar_com_undo(
        &mut self,
        linha: usize,
        coluna: usize,
    ) -> Result<EdicaoMesclagem, ErroTabela> {
        let antes = self.mesclagens.clone();
        self.desmesclar(linha, coluna)?;
        Ok(EdicaoMesclagem {
            antes,
            depois: self.mesclagens.clone(),
        })
    }

    /// Divide uma única mesclagem em duas partes adjacentes; partes 1×1
    /// passam a ser células comuns. `em` é índice absoluto de linha/coluna
    /// e deve estar estritamente dentro da região de origem.
    pub fn dividir_mesclagem(
        &mut self,
        linha: usize,
        coluna: usize,
        direcao: DivisaoMesclagem,
        em: usize,
    ) -> Result<EdicaoMesclagem, ErroTabela> {
        let indice = self.indice_mesclagem(linha, coluna)?;
        let r = self.mesclagens[indice];
        let (a, b) = match direcao {
            DivisaoMesclagem::Horizontal if em > r.linha && em < r.linha + r.linhas => (
                Regiao { linhas: em - r.linha, ..r },
                Regiao {
                    linha: em,
                    linhas: r.linha + r.linhas - em,
                    ..r
                },
            ),
            DivisaoMesclagem::Vertical if em > r.coluna && em < r.coluna + r.colunas => (
                Regiao { colunas: em - r.coluna, ..r },
                Regiao {
                    coluna: em,
                    colunas: r.coluna + r.colunas - em,
                    ..r
                },
            ),
            _ => return Err(ErroTabela::MesclagemInvalida),
        };
        let antes = self.mesclagens.clone();
        let mut depois = antes.clone();
        depois.remove(indice);
        for parte in [a, b] {
            if parte.linhas > 1 || parte.colunas > 1 {
                depois.push(parte);
            }
        }
        self.aplicar_mesclagens(depois)?;
        Ok(EdicaoMesclagem {
            antes,
            depois: self.mesclagens.clone(),
        })
    }

    /// Reagrupa partes mescladas previamente divididas, sem perder âncoras
    /// preenchidas. Recusa qualquer região apenas parcialmente interceptada.
    pub fn remesclar(&mut self, r: Regiao) -> Result<(), ErroTabela> {
        self.validar_regiao_mesclada(r)?;
        if self
            .mesclagens
            .iter()
            .any(|m| m.intersecta(r) && !r.contem_regiao(*m))
        {
            return Err(ErroTabela::Sobreposicao);
        }
        let mut depois: Vec<Regiao> = self
            .mesclagens
            .iter()
            .copied()
            .filter(|m| !r.contem_regiao(*m))
            .collect();
        depois.push(r);
        self.aplicar_mesclagens(depois)
    }

    pub fn remesclar_com_undo(&mut self, r: Regiao) -> Result<EdicaoMesclagem, ErroTabela> {
        let antes = self.mesclagens.clone();
        self.remesclar(r)?;
        Ok(EdicaoMesclagem {
            antes,
            depois: self.mesclagens.clone(),
        })
    }

    pub fn definir_largura(&mut self, coluna: usize, w: f32) -> Result<(), ErroTabela> {
        if coluna >= self.colunas || !w.is_finite() || w < 0.0 {
            return Err(ErroTabela::ForaDosLimites);
        }
        self.larguras.insert(coluna, w);
        Ok(())
    }
    pub fn definir_altura(&mut self, linha: usize, h: f32) -> Result<(), ErroTabela> {
        if linha >= self.linhas || !h.is_finite() || h < 0.0 {
            return Err(ErroTabela::ForaDosLimites);
        }
        self.alturas.insert(linha, h);
        Ok(())
    }
    /// Restaura a largura padrão, removendo a dimensão personalizada.
    pub fn limpar_largura(&mut self, coluna: usize) -> Result<(), ErroTabela> {
        if coluna >= self.colunas {
            return Err(ErroTabela::ForaDosLimites);
        }
        self.larguras.remove(&coluna);
        Ok(())
    }
    /// Restaura a altura padrão, removendo a dimensão personalizada.
    pub fn limpar_altura(&mut self, linha: usize) -> Result<(), ErroTabela> {
        if linha >= self.linhas {
            return Err(ErroTabela::ForaDosLimites);
        }
        self.alturas.remove(&linha);
        Ok(())
    }
    pub fn largura(&self, coluna: usize) -> Option<f32> {
        self.larguras.get(&coluna).copied()
    }
    pub fn altura(&self, linha: usize) -> Option<f32> {
        self.alturas.get(&linha).copied()
    }
}

#[derive(Debug, Clone, Copy)]
enum Eixo {
    Linha,
    Coluna,
}

/// Reindexação sem alocar pelas dimensões: custo proporcional aos itens usados.
fn reindexar(i: usize, ponto: usize, quantidade: usize, inserir: bool) -> Option<usize> {
    if inserir {
        Some(if i >= ponto { i + quantidade } else { i })
    } else if i < ponto {
        Some(i)
    } else if i < ponto + quantidade {
        None
    } else {
        Some(i - quantidade)
    }
}

impl Tabela {
    fn validar_posicao(&self, linha: usize, coluna: usize) -> Result<(), ErroTabela> {
        if linha >= self.linhas || coluna >= self.colunas {
            return Err(ErroTabela::ForaDosLimites);
        }
        if self.mesclagens.iter().any(|m| {
            linha >= m.linha
                && coluna >= m.coluna
                && linha < m.linha + m.linhas
                && coluna < m.coluna + m.colunas
                && (linha, coluna) != (m.linha, m.coluna)
        }) {
            return Err(ErroTabela::Sobreposicao);
        }
        Ok(())
    }

    /// Alteração apenas do valor, sem reconstruir estilos básicos ou avançados.
    pub fn atualizar_valor(
        &mut self,
        linha: usize,
        coluna: usize,
        valor: Valor,
    ) -> Result<(), ErroTabela> {
        self.validar_posicao(linha, coluna)?;
        let mut celula = self.obter(linha, coluna).cloned().unwrap_or_default();
        celula.valor = valor;
        self.definir(linha, coluna, celula)
    }

    /// Substitui o estilo básico conservando o valor e o layout complementar.
    pub fn atualizar_estilo(
        &mut self,
        linha: usize,
        coluna: usize,
        estilo: EstiloCelula,
    ) -> Result<(), ErroTabela> {
        self.validar_posicao(linha, coluna)?;
        let mut celula = self.obter(linha, coluna).cloned().unwrap_or_default();
        celula.estilo = estilo;
        self.definir(linha, coluna, celula)
    }

    /// Propriedades de layout associadas a uma célula (inclusive vazia).
    pub fn layout(&self, linha: usize, coluna: usize) -> Option<&EstiloLayout> {
        self.estilos_layout.get(&(linha, coluna))
    }

    /// Rejeita NaN/infinitos, padding negativo ou entrelinha não positiva.
    /// Erros não modificam o conteúdo nem os estilos existentes.
    pub fn definir_layout(
        &mut self,
        linha: usize,
        coluna: usize,
        layout: EstiloLayout,
    ) -> Result<(), ErroTabela> {
        self.validar_posicao(linha, coluna)?;
        if !layout.valido() {
            return Err(ErroTabela::ForaDosLimites);
        }
        if layout == EstiloLayout::default() {
            self.estilos_layout.remove(&(linha, coluna));
        } else {
            self.estilos_layout.insert((linha, coluna), layout);
        }
        Ok(())
    }

    pub fn estilo_completo(
        &self,
        linha: usize,
        coluna: usize,
    ) -> Result<EstiloCompleto, ErroTabela> {
        if linha >= self.linhas || coluna >= self.colunas {
            return Err(ErroTabela::ForaDosLimites);
        }
        Ok(EstiloCompleto {
            basico: self
                .obter(linha, coluna)
                .map(|c| c.estilo.clone())
                .unwrap_or_default(),
            layout: self.layout(linha, coluna).cloned().unwrap_or_default(),
        })
    }

    /// Preserva o valor; aplica basic + layout como uma alteração validada.
    pub fn definir_estilo_completo(
        &mut self,
        linha: usize,
        coluna: usize,
        estilo: EstiloCompleto,
    ) -> Result<(), ErroTabela> {
        self.validar_posicao(linha, coluna)?;
        if !estilo.layout.valido() {
            return Err(ErroTabela::ForaDosLimites);
        }
        self.atualizar_estilo(linha, coluna, estilo.basico)?;
        self.definir_layout(linha, coluna, estilo.layout)
    }

    pub fn politica_bordas(&self) -> PoliticaBordas {
        self.politica_bordas
    }
    pub fn definir_politica_bordas(&mut self, politica: PoliticaBordas) {
        self.politica_bordas = politica;
    }

    fn borda_da_celula(&self, linha: usize, coluna: usize, lado: Lado) -> BordaVisual {
        let base = self
            .obter(linha, coluna)
            .map(|c| c.estilo.bordas[lado.indice()])
            .unwrap_or_default();
        let largura = self
            .layout(linha, coluna)
            .and_then(|l| l.larguras_bordas[lado.indice()])
            .unwrap_or(f32::from(base.espessura));
        BordaVisual {
            estilo: base.estilo,
            espessura: largura,
            cor: base.cor,
        }
    }

    /// Em modo colapsado, resolve a borda em comum pelo maior traço visível,
    /// maior espessura, prioridade sólido > tracejado > pontilhado e,
    /// em empate, pela célula superior/esquerda. Não altera o armazenamento.
    pub fn borda_visivel(
        &self,
        linha: usize,
        coluna: usize,
        lado: Lado,
    ) -> Result<BordaVisual, ErroTabela> {
        if linha >= self.linhas || coluna >= self.colunas {
            return Err(ErroTabela::ForaDosLimites);
        }
        let atual = self.borda_da_celula(linha, coluna, lado);
        if self.politica_bordas == PoliticaBordas::Separadas {
            return Ok(atual);
        }
        let vizinho = match lado {
            Lado::Cima => linha.checked_sub(1).map(|l| (l, coluna)),
            Lado::Direita => coluna
                .checked_add(1)
                .filter(|&c| c < self.colunas)
                .map(|c| (linha, c)),
            Lado::Baixo => linha
                .checked_add(1)
                .filter(|&l| l < self.linhas)
                .map(|l| (l, coluna)),
            Lado::Esquerda => coluna.checked_sub(1).map(|c| (linha, c)),
        };
        let Some((l, c)) = vizinho else {
            return Ok(atual);
        };
        let outra = self.borda_da_celula(l, c, lado.oposto());
        let p = atual.prioridade();
        let q = outra.prioridade();
        let largura_atual = if p.0 == 0 { 0.0 } else { atual.espessura };
        let largura_outra = if q.0 == 0 { 0.0 } else { outra.espessura };
        let ganha_atual = if p.0 != q.0 {
            p.0 > q.0
        } else if largura_atual != largura_outra {
            largura_atual > largura_outra
        } else if p.1 != q.1 {
            p.1 > q.1
        } else {
            (linha, coluna) < (l, c)
        };
        Ok(if ganha_atual { atual } else { outra })
    }

    pub fn inserir_linhas(&mut self, indice: usize, quantidade: usize) -> Result<(), ErroTabela> {
        self.editar_eixo(Eixo::Linha, indice, quantidade, true)
    }
    pub fn excluir_linhas(&mut self, indice: usize, quantidade: usize) -> Result<(), ErroTabela> {
        self.editar_eixo(Eixo::Linha, indice, quantidade, false)
    }
    pub fn inserir_colunas(&mut self, indice: usize, quantidade: usize) -> Result<(), ErroTabela> {
        self.editar_eixo(Eixo::Coluna, indice, quantidade, true)
    }
    pub fn excluir_colunas(&mut self, indice: usize, quantidade: usize) -> Result<(), ErroTabela> {
        self.editar_eixo(Eixo::Coluna, indice, quantidade, false)
    }

    /// Expandir é sempre permitido; encolher não pode descartar células,
    /// layouts, dimensões customizadas nem parte de uma mesclagem.
    pub fn redimensionar(&mut self, linhas: usize, colunas: usize) -> Result<(), ErroTabela> {
        if self
            .celulas
            .keys()
            .chain(self.estilos_layout.keys())
            .any(|&(l, c)| l >= linhas || c >= colunas)
            || self.larguras.keys().any(|&c| c >= colunas)
            || self.alturas.keys().any(|&l| l >= linhas)
            || self
                .mesclagens
                .iter()
                .any(|r| r.linha + r.linhas > linhas || r.coluna + r.colunas > colunas)
        {
            return Err(ErroTabela::ForaDosLimites);
        }
        self.linhas = linhas;
        self.colunas = colunas;
        Ok(())
    }

    /// Exclusão parcial de uma mesclagem é recusada sem alterações. Inclusões
    /// no interior da mesclagem a expandem; inclusões no início a deslocam.
    /// Índices de itens esparsos, larguras, alturas e layouts são preservados.
    fn editar_eixo(
        &mut self,
        eixo: Eixo,
        ponto: usize,
        quantidade: usize,
        inserir: bool,
    ) -> Result<(), ErroTabela> {
        let limite = match eixo {
            Eixo::Linha => self.linhas,
            Eixo::Coluna => self.colunas,
        };
        if ponto > limite {
            return Err(ErroTabela::ForaDosLimites);
        }
        if !inserir
            && ponto
                .checked_add(quantidade)
                .map_or(true, |end| end > limite)
        {
            return Err(ErroTabela::ForaDosLimites);
        }
        if quantidade == 0 {
            return Ok(());
        }
        let novo = if inserir {
            limite
                .checked_add(quantidade)
                .ok_or(ErroTabela::ForaDosLimites)?
        } else {
            limite - quantidade
        };
        if !inserir {
            let fim_remocao = ponto + quantidade;
            if self.mesclagens.iter().any(|r| {
                let (inicio, tam) = match eixo {
                    Eixo::Linha => (r.linha, r.linhas),
                    Eixo::Coluna => (r.coluna, r.colunas),
                };
                let fim = inicio + tam;
                let intercepta = ponto < fim && inicio < fim_remocao;
                let removida_toda = ponto <= inicio && fim_remocao >= fim;
                intercepta && !removida_toda
            }) {
                return Err(ErroTabela::MesclagemInvalida);
            }
        }
        let mapear = |linha: usize, coluna: usize| -> Option<(usize, usize)> {
            match eixo {
                Eixo::Linha => reindexar(linha, ponto, quantidade, inserir).map(|n| (n, coluna)),
                Eixo::Coluna => reindexar(coluna, ponto, quantidade, inserir).map(|n| (linha, n)),
            }
        };
        self.celulas = std::mem::take(&mut self.celulas)
            .into_iter()
            .filter_map(|((l, c), cel)| mapear(l, c).map(|pos| (pos, cel)))
            .collect();
        self.estilos_layout = std::mem::take(&mut self.estilos_layout)
            .into_iter()
            .filter_map(|((l, c), estilo)| mapear(l, c).map(|pos| (pos, estilo)))
            .collect();
        let medidas = match eixo {
            Eixo::Linha => &mut self.alturas,
            Eixo::Coluna => &mut self.larguras,
        };
        *medidas = std::mem::take(medidas)
            .into_iter()
            .filter_map(|(i, v)| reindexar(i, ponto, quantidade, inserir).map(|n| (n, v)))
            .collect();
        self.mesclagens = std::mem::take(&mut self.mesclagens)
            .into_iter()
            .filter_map(|mut r| {
                let (inicio, tam) = match eixo {
                    Eixo::Linha => (&mut r.linha, &mut r.linhas),
                    Eixo::Coluna => (&mut r.coluna, &mut r.colunas),
                };
                let fim = *inicio + *tam;
                if inserir {
                    if ponto <= *inicio {
                        *inicio += quantidade;
                    } else if ponto < fim {
                        *tam += quantidade;
                    }
                } else if ponto <= *inicio && ponto + quantidade >= fim {
                    return None;
                } else if ponto + quantidade <= *inicio {
                    *inicio -= quantidade;
                }
                Some(r)
            })
            .collect();
        match eixo {
            Eixo::Linha => self.linhas = novo,
            Eixo::Coluna => self.colunas = novo,
        };
        Ok(())
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    #[test]
    fn modelo_esparso_com_estilo() {
        let mut t = Tabela::nova(1_000_000, 12);
        t.definir(
            10,
            2,
            Celula {
                valor: Valor::Texto("Feather".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(t.iter().count(), 1);
        assert_eq!(t.obter(10, 2).unwrap().valor.texto(), "Feather");
        assert_eq!(
            t.definir(1_000_000, 0, Celula::default()),
            Err(ErroTabela::ForaDosLimites)
        );
    }
    #[test]
    fn mesclar_sem_perder_dados() {
        let mut t = Tabela::nova(3, 3);
        let a = Regiao {
            linha: 0,
            coluna: 0,
            linhas: 2,
            colunas: 2,
        };
        t.definir(
            1,
            1,
            Celula {
                valor: Valor::Numero(3.0),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(t.mesclar(a), Err(ErroTabela::DadosNaMesclagem));
        t.definir(1, 1, Celula::default()).unwrap();
        t.mesclar(a).unwrap();
        assert_eq!(t.mesclar(a), Err(ErroTabela::Sobreposicao));
        assert_eq!(
            t.definir(1, 1, Celula::default()),
            Err(ErroTabela::Sobreposicao)
        );
    }
}

#[cfg(test)]
mod testes_edicao_e_estilos {
    use super::*;

    fn preenchida(valor: &str) -> Celula {
        Celula {
            valor: Valor::Texto(valor.into()),
            estilo: EstiloCelula {
                fonte: Some("Inter".into()),
                tamanho: Some(12.5),
                negrito: true,
                fundo: Some([10, 20, 30, 255]),
                texto: Some([240, 230, 220, 255]),
                ..Default::default()
            },
        }
    }

    #[test]
    fn estilo_completo_preserva_valor_fonte_cores_e_layout() {
        let mut t = Tabela::nova(3, 3);
        t.definir(0, 0, preenchida("Original")).unwrap();
        let base = t.obter(0, 0).unwrap().estilo.clone();
        let mut formato = EstiloCompleto {
            basico: base.clone(),
            layout: EstiloLayout::default(),
        };
        formato.layout.horizontal = AlinhamentoHorizontal::Justificado;
        formato.layout.vertical = AlinhamentoVertical::Inferior;
        formato.layout.entre_linhas = 1.5;
        formato.layout.entre_paragrafos = 3.25;
        formato.layout.padding = Espacamento {
            cima: 1.0,
            direita: 2.0,
            baixo: 3.0,
            esquerda: 4.0,
        };
        formato.layout.larguras_bordas[1] = Some(1.75);
        t.definir_estilo_completo(0, 0, formato.clone()).unwrap();
        assert_eq!(t.obter(0, 0).unwrap().valor.texto(), "Original");
        assert_eq!(t.estilo_completo(0, 0).unwrap(), formato);
        t.atualizar_valor(0, 0, Valor::Numero(42.0)).unwrap();
        assert_eq!(t.estilo_completo(0, 0).unwrap(), formato);
        t.atualizar_estilo(0, 0, base).unwrap();
        assert_eq!(t.layout(0, 0).unwrap(), &formato.layout);
        t.atualizar_valor(0, 0, Valor::Vazio).unwrap();
        assert_eq!(t.estilo_completo(0, 0).unwrap(), formato);
    }

    #[test]
    fn estilos_em_celulas_vazias_e_validacao_transacional() {
        let mut t = Tabela::nova(2, 2);
        let mut layout = EstiloLayout::default();
        layout.padding.esquerda = 10.0;
        t.definir_layout(1, 1, layout.clone()).unwrap();
        assert!(t.obter(1, 1).is_none());
        assert_eq!(t.estilo_completo(1, 1).unwrap().layout, layout);
        let snapshot = t.clone();
        let mut invalido = layout.clone();
        invalido.entre_linhas = 0.0;
        assert_eq!(
            t.definir_layout(1, 1, invalido),
            Err(ErroTabela::ForaDosLimites)
        );
        let mut invalido = layout.clone();
        invalido.larguras_bordas[0] = Some(f32::NAN);
        assert_eq!(
            t.definir_layout(1, 1, invalido),
            Err(ErroTabela::ForaDosLimites)
        );
        assert_eq!(t, snapshot);
        assert_eq!(
            t.definir_layout(5, 5, layout),
            Err(ErroTabela::ForaDosLimites)
        );
    }

    #[test]
    fn bordas_colapsadas_prioridade_espessura_e_desempate() {
        let mut t = Tabela::nova(1, 2);
        let mut esquerda = Celula::default();
        esquerda.estilo.bordas[1] = Borda {
            estilo: Traco::Tracejado,
            espessura: 2,
            cor: [255, 0, 0, 255],
        };
        let mut direita = Celula::default();
        direita.estilo.bordas[3] = Borda {
            estilo: Traco::Solido,
            espessura: 1,
            cor: [0, 0, 255, 255],
        };
        t.definir(0, 0, esquerda).unwrap();
        t.definir(0, 1, direita).unwrap();
        assert_eq!(t.borda_visivel(0, 0, Lado::Direita).unwrap().espessura, 2.0);
        assert_eq!(
            t.borda_visivel(0, 1, Lado::Esquerda).unwrap().espessura,
            1.0
        );
        t.definir_politica_bordas(PoliticaBordas::Colapsadas);
        assert_eq!(
            t.borda_visivel(0, 0, Lado::Direita).unwrap().cor,
            [255, 0, 0, 255]
        );
        assert_eq!(
            t.borda_visivel(0, 0, Lado::Direita),
            t.borda_visivel(0, 1, Lado::Esquerda)
        );
        let mut l = EstiloLayout::default();
        l.larguras_bordas[3] = Some(2.0);
        t.definir_layout(0, 1, l).unwrap();
        // Empate na largura: sólido tem prioridade.
        assert_eq!(
            t.borda_visivel(0, 0, Lado::Direita).unwrap().cor,
            [0, 0, 255, 255]
        );
        let mut base = t.obter(0, 1).unwrap().estilo.clone();
        base.bordas[3].estilo = Traco::Tracejado;
        t.atualizar_estilo(0, 1, base).unwrap();
        // Empate também no traço: vence a célula à esquerda.
        assert_eq!(
            t.borda_visivel(0, 1, Lado::Esquerda).unwrap().cor,
            [255, 0, 0, 255]
        );
    }

    #[test]
    fn insercoes_exclusoes_preservam_estilos_e_dimensoes() {
        let mut t = Tabela::nova(10, 8);
        t.definir(4, 3, preenchida("Centro")).unwrap();
        let estilo_original = t.obter(4, 3).unwrap().estilo.clone();
        let mut layout = EstiloLayout {
            vertical: AlinhamentoVertical::Centro,
            ..EstiloLayout::default()
        };
        layout.padding.cima = 8.0;
        t.definir_layout(4, 3, layout.clone()).unwrap();
        t.definir_altura(4, 27.0).unwrap();
        t.definir_largura(3, 125.0).unwrap();
        t.inserir_linhas(2, 2).unwrap();
        t.inserir_colunas(1, 2).unwrap();
        assert_eq!(t.dimensoes(), (12, 10));
        assert_eq!(t.obter(6, 5).unwrap().valor.texto(), "Centro");
        assert_eq!(t.estilo_completo(6, 5).unwrap().basico, estilo_original);
        assert_eq!(t.layout(6, 5), Some(&layout));
        assert_eq!(t.altura(6), Some(27.0));
        assert_eq!(t.largura(5), Some(125.0));
        t.excluir_linhas(0, 1).unwrap();
        t.excluir_colunas(0, 1).unwrap();
        assert_eq!(t.dimensoes(), (11, 9));
        assert_eq!(t.obter(5, 4).unwrap().valor.texto(), "Centro");
        assert_eq!(t.estilo_completo(5, 4).unwrap().basico, estilo_original);
        assert_eq!(t.layout(5, 4), Some(&layout));
        assert_eq!(t.altura(5), Some(27.0));
        assert_eq!(t.largura(4), Some(125.0));
    }

    #[test]
    fn insercao_em_mesclagem_expande_e_exclusao_parcial_e_atomica() {
        let mut t = Tabela::nova(6, 6);
        let r = Regiao {
            linha: 1,
            coluna: 1,
            linhas: 2,
            colunas: 2,
        };
        t.definir(1, 1, preenchida("Mesclada")).unwrap();
        t.mesclar(r).unwrap();
        t.inserir_linhas(2, 1).unwrap();
        t.inserir_colunas(2, 1).unwrap();
        assert_eq!(
            t.mesclagens(),
            &[Regiao {
                linha: 1,
                coluna: 1,
                linhas: 3,
                colunas: 3,
            }]
        );
        let antes = t.clone();
        assert_eq!(t.excluir_linhas(2, 1), Err(ErroTabela::MesclagemInvalida));
        assert_eq!(t.excluir_colunas(2, 1), Err(ErroTabela::MesclagemInvalida));
        assert_eq!(t, antes);
        assert_eq!(t.obter(1, 1).unwrap().valor.texto(), "Mesclada");
        assert_eq!(
            t.definir_layout(2, 2, EstiloLayout::default()),
            Err(ErroTabela::Sobreposicao)
        );
        t.excluir_linhas(1, 3).unwrap();
        assert!(t.mesclagens().is_empty());
        assert_eq!(t.dimensoes(), (4, 7));
    }

    #[test]
    fn mesclar_nao_pode_descartar_formato_de_celula_vazia() {
        let mut t = Tabela::nova(3, 3);
        let mut style = EstiloLayout::default();
        style.padding.cima = 3.0;
        t.definir_layout(1, 1, style).unwrap();
        assert_eq!(
            t.mesclar(Regiao {
                linha: 0,
                coluna: 0,
                linhas: 2,
                colunas: 2,
            }),
            Err(ErroTabela::DadosNaMesclagem)
        );
    }

    #[test]
    fn redimensionar_sem_perder_celulas_layout_ou_larguras() {
        let mut t = Tabela::nova(2, 2);
        let mut estilo = EstiloLayout::default();
        estilo.padding.cima = 5.0;
        t.definir_layout(1, 1, estilo.clone()).unwrap();
        t.redimensionar(200_000, 200_000).unwrap();
        assert_eq!(t.iter().count(), 0);
        assert_eq!(t.redimensionar(1, 1), Err(ErroTabela::ForaDosLimites));
        assert_eq!(t.dimensoes(), (200_000, 200_000));
        t.definir_layout(1, 1, EstiloLayout::default()).unwrap();
        t.definir_largura(100, 45.0).unwrap();
        assert_eq!(t.redimensionar(1, 1), Err(ErroTabela::ForaDosLimites));
        t.definir_largura(100, 0.0).unwrap();
        // Dimensão customizada igual a zero ainda é propriedade persistida.
        assert_eq!(t.redimensionar(1, 1), Err(ErroTabela::ForaDosLimites));
        t.limpar_largura(100).unwrap();
        t.redimensionar(1, 1).unwrap();
        assert_eq!(t.dimensoes(), (1, 1));
        assert_eq!(t.estilo_completo(0, 0).unwrap(), EstiloCompleto::default());
    }

    #[test]
    fn indices_invalidos_e_overflow_nao_modificam_tabela() {
        let mut t = Tabela::nova(2, 2);
        t.definir(1, 1, preenchida("Intacta")).unwrap();
        let inicial = t.clone();
        assert_eq!(
            t.inserir_linhas(0, usize::MAX),
            Err(ErroTabela::ForaDosLimites)
        );
        assert_eq!(
            t.inserir_colunas(0, usize::MAX),
            Err(ErroTabela::ForaDosLimites)
        );
        assert_eq!(t.excluir_linhas(1, 2), Err(ErroTabela::ForaDosLimites));
        assert_eq!(t.excluir_colunas(3, 1), Err(ErroTabela::ForaDosLimites));
        assert_eq!(t, inicial);
        t.inserir_linhas(2, 0).unwrap();
        t.excluir_colunas(2, 0).unwrap();
        assert_eq!(t, inicial);
    }

    #[test]
    fn mesclagens_e_estilos_sao_deslocados_sem_recorrer_a_grade_densa() {
        let mut t = Tabela::nova(1_000_000, 1_000_000);
        let r = Regiao {
            linha: 800_000,
            coluna: 700_000,
            linhas: 2,
            colunas: 2,
        };
        t.definir(800_000, 700_000, preenchida("Longe")).unwrap();
        let e = EstiloLayout {
            entre_paragrafos: 4.5,
            ..EstiloLayout::default()
        };
        t.definir_layout(800_000, 700_000, e.clone()).unwrap();
        t.mesclar(r).unwrap();
        t.inserir_linhas(500_000, 2).unwrap();
        t.inserir_colunas(400_000, 3).unwrap();
        assert_eq!(
            t.mesclagens(),
            &[Regiao {
                linha: 800_002,
                coluna: 700_003,
                linhas: 2,
                colunas: 2,
            }]
        );
        assert_eq!(t.obter(800_002, 700_003).unwrap().valor.texto(), "Longe");
        assert_eq!(t.layout(800_002, 700_003), Some(&e));
        assert_eq!(t.iter().count(), 1);
    }
}
