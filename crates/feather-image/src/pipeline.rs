//! Pipeline de edições não destrutivas e histórico reversível de RGBA8.
//!
//! Domínio puro: o aplicativo controla atalhos/UI, serialização e armazenamento.
//! `PipelineImagem` descreve operações imutáveis; `HistoricoImagem` mantém um
//! cursor undo/redo e snapshots LRU limitados, sem alterar a imagem original.

use crate::{
    aplicar_cores_rgba, AjustesCor, ErroCor, ErroEfeito, ErroImagem, ErroMascara,
    ErroVisual, ImagemRgba, Interpolacao, MascaraAlfa, OrientacaoExif,
};
use std::{collections::VecDeque, sync::Arc};

const CACHE_PADRAO_BYTES: usize = 32 * 1024 * 1024;

/// Erros de edição preservam a origem e não alteram o índice undo/redo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErroPipeline {
    Imagem(ErroImagem),
    Cores(ErroCor),
    Efeito(ErroEfeito),
    Mascara(ErroMascara),
    Visual(ErroVisual),
    ParametroInvalido,
}
impl From<ErroImagem> for ErroPipeline {
    fn from(e: ErroImagem) -> Self { Self::Imagem(e) }
}
impl From<ErroCor> for ErroPipeline {
    fn from(e: ErroCor) -> Self { Self::Cores(e) }
}
impl From<ErroEfeito> for ErroPipeline {
    fn from(e: ErroEfeito) -> Self { Self::Efeito(e) }
}
impl From<ErroMascara> for ErroPipeline {
    fn from(e: ErroMascara) -> Self { Self::Mascara(e) }
}
impl From<ErroVisual> for ErroPipeline {
    fn from(e: ErroVisual) -> Self { Self::Visual(e) }
}

/// Operações declarativas. Todas produzem um novo buffer; nunca mutam a fonte.
/// Para composições em camadas, sombras e glow devem ser mantidos pelo app.
#[derive(Debug, Clone, PartialEq)]
pub enum OperacaoImagem {
    EspelharHorizontal,
    EspelharVertical,
    TonsDeCinza,
    AjustarLuz { brilho: f32, contraste: f32 },
    AjustarCores(AjustesCor),
    Cortar { x: u32, y: u32, largura: u32, altura: u32 },
    Redimensionar { largura: u32, altura: u32, interpolacao: Option<Interpolacao> },
    Desfocar { raio: u32 },
    ReduzirRuido { raio: u8 },
    Nitidez { intensidade: f32 },
    Orientar(OrientacaoExif),
    AplicarMascara(MascaraAlfa),
    Opacidade { fator: f32 },
    Perspectiva { intensidade: f32 },
}

impl OperacaoImagem {
    /// Executa a operação isolada; falhas nunca modificam `imagem`.
    pub fn executar(&self, imagem: &ImagemRgba) -> Result<ImagemRgba, ErroPipeline> {
        match self {
            Self::EspelharHorizontal => {
                let mut saida = imagem.clone();
                saida.espelhar_horizontal();
                Ok(saida)
            }
            Self::EspelharVertical => {
                let mut saida = imagem.clone();
                saida.espelhar_vertical();
                Ok(saida)
            }
            Self::TonsDeCinza => {
                let mut saida = imagem.clone();
                saida.tons_de_cinza();
                Ok(saida)
            }
            Self::AjustarLuz { brilho, contraste } => {
                if !brilho.is_finite() || !contraste.is_finite()
                    || !(-1.0..=1.0).contains(brilho)
                    || !(-1.0..=1.0).contains(contraste)
                {
                    return Err(ErroPipeline::ParametroInvalido);
                }
                let mut saida = imagem.clone();
                saida.ajustar_luz(*brilho, *contraste);
                Ok(saida)
            }
            Self::AjustarCores(ajustes) => {
                let (largura, altura) = imagem.dimensoes();
                let mut pixels = imagem.pixels().to_vec();
                aplicar_cores_rgba(&mut pixels, *ajustes)?;
                Ok(ImagemRgba::nova(largura, altura, pixels)?)
            }
            Self::Cortar { x, y, largura, altura } =>
                Ok(imagem.cortar(*x, *y, *largura, *altura)?),
            Self::Redimensionar { largura, altura, interpolacao } => {
                match interpolacao {
                    Some(i) => Ok(imagem.redimensionar_interpolado(*largura, *altura, *i)?),
                    None => Ok(imagem.redimensionar_vizinho(*largura, *altura)?),
                }
            }
            Self::Desfocar { raio } => Ok(imagem.desfocar(*raio)),
            Self::ReduzirRuido { raio } => Ok(imagem.reduzir_ruido(*raio)),
            Self::Nitidez { intensidade } => {
                let mut saida = imagem.clone();
                saida.realcar_detalhes(*intensidade)?;
                Ok(saida)
            }
            Self::Orientar(orientacao) => Ok(imagem.orientar_exif(*orientacao)?),
            Self::AplicarMascara(mascara) => Ok(mascara.aplicar(imagem)?),
            Self::Opacidade { fator } => Ok(imagem.aplicar_opacidade(*fator)?),
            Self::Perspectiva { intensidade } =>
                Ok(imagem.aplicar_perspectiva(*intensidade)?),
        }
    }
}

/// Descrição imutável de operações, compartilhando a origem via `Arc`.
///
/// A função `com_operacao` retorna um novo pipeline, sem mutar os comandos
/// anteriores. Nenhum cache/GUI é acoplado a este tipo.
#[derive(Debug, Clone)]
pub struct PipelineImagem {
    origem: Arc<ImagemRgba>,
    operacoes: Vec<OperacaoImagem>,
}

impl PipelineImagem {
    pub fn novo(origem: ImagemRgba) -> Self {
        Self { origem: Arc::new(origem), operacoes: Vec::new() }
    }
    /// Retorna nova descrição; a anterior e os dados originais permanecem vivos.
    pub fn com_operacao(&self, operacao: OperacaoImagem) -> Self {
        let mut proximo = self.clone();
        proximo.operacoes.push(operacao);
        proximo
    }
    pub fn origem(&self) -> &ImagemRgba { &self.origem }
    pub fn operacoes(&self) -> &[OperacaoImagem] { &self.operacoes }
    /// Renderiza a descrição sempre do original: determinístico e sem side effects.
    pub fn executar(&self) -> Result<ImagemRgba, ErroPipeline> {
        let mut atual = self.origem.as_ref().clone();
        for op in &self.operacoes {
            atual = op.executar(&atual)?;
        }
        Ok(atual)
    }
}

/// Histórico mantido no adaptador de consumidor (Slide/Docs/Sheet).
///
/// O cache LRU conta exclusivamente snapshots internos, não a fonte ou as
/// imagens `Arc` já entregues ao chamador. `limite_cache_bytes = 0` desliga
/// o cache, sem desligar undo/redo: etapas são reexecutadas sob demanda.
#[derive(Debug)]
pub struct HistoricoImagem {
    origem: Arc<ImagemRgba>,
    operacoes: Vec<OperacaoImagem>,
    cursor: usize,
    cache: VecDeque<(usize, Arc<ImagemRgba>)>,
    cache_bytes: usize,
    limite_cache_bytes: usize,
}

impl HistoricoImagem {
    pub fn novo(origem: ImagemRgba) -> Self {
        Self::com_limite_cache(origem, CACHE_PADRAO_BYTES)
    }

    pub fn com_limite_cache(origem: ImagemRgba, limite_cache_bytes: usize) -> Self {
        Self {
            origem: Arc::new(origem),
            operacoes: Vec::new(),
            cursor: 0,
            cache: VecDeque::new(),
            cache_bytes: 0,
            limite_cache_bytes,
        }
    }

    pub fn origem(&self) -> &ImagemRgba { &self.origem }
    pub fn operacoes(&self) -> &[OperacaoImagem] { &self.operacoes }
    pub fn indice(&self) -> usize { self.cursor }
    pub fn pode_desfazer(&self) -> bool { self.cursor > 0 }
    pub fn pode_refazer(&self) -> bool { self.cursor < self.operacoes.len() }
    pub fn bytes_cache(&self) -> usize { self.cache_bytes }
    pub fn quantidade_snapshots(&self) -> usize { self.cache.len() }
    pub fn limite_cache_bytes(&self) -> usize { self.limite_cache_bytes }

    pub fn configurar_limite_cache(&mut self, limite: usize) {
        self.limite_cache_bytes = limite;
        self.ajustar_cache();
    }

    /// Snapshot declarativo correspondente ao cursor, útil para salvar
    /// operações no documento sem persistir os buffers em cache.
    pub fn pipeline_atual(&self) -> PipelineImagem {
        PipelineImagem {
            origem: Arc::clone(&self.origem),
            operacoes: self.operacoes[..self.cursor].to_vec(),
        }
    }

    fn ajustar_cache(&mut self) {
        while self.cache_bytes > self.limite_cache_bytes {
            if let Some((_, imagem)) = self.cache.pop_front() {
                self.cache_bytes -= imagem.pixels().len();
            } else {
                self.cache_bytes = 0;
                break;
            }
        }
    }

    fn guardar(&mut self, etapa: usize, imagem: Arc<ImagemRgba>) {
        let bytes = imagem.pixels().len();
        if bytes > self.limite_cache_bytes || self.limite_cache_bytes == 0 {
            return;
        }
        if let Some(p) = self.cache.iter().position(|(idx, _)| *idx == etapa) {
            if let Some((_, anterior)) = self.cache.remove(p) {
                self.cache_bytes -= anterior.pixels().len();
            }
        }
        self.cache_bytes = self.cache_bytes.saturating_add(bytes);
        self.cache.push_back((etapa, imagem));
        self.ajustar_cache();
    }

    /// Reusa a etapa em cache mais avançada que não ultrapassa o cursor.
    /// Move a etapa acessada para o fim do LRU e armazena a saída se couber.
    pub fn resultado(&mut self) -> Result<Arc<ImagemRgba>, ErroPipeline> {
        if self.cursor == 0 {
            return Ok(Arc::clone(&self.origem));
        }
        let mut melhor: Option<(usize, usize)> = None;
        for (pos, (etapa, _)) in self.cache.iter().enumerate() {
            if *etapa <= self.cursor && melhor.map_or(true, |(melhor_etapa, _)| *etapa > melhor_etapa) {
                melhor = Some((*etapa, pos));
            }
        }
        let (mut etapa, mut atual) = if let Some((indice, pos)) = melhor {
            let (_, imagem) = self.cache.remove(pos).expect("indice de cache válido");
            let atual = Arc::clone(&imagem);
            self.cache.push_back((indice, imagem));
            (indice, atual)
        } else {
            (0, Arc::clone(&self.origem))
        };

        while etapa < self.cursor {
            atual = Arc::new(self.operacoes[etapa].executar(&atual)?);
            etapa += 1;
        }
        self.guardar(self.cursor, Arc::clone(&atual));
        Ok(atual)
    }

    /// Aplica atomicamente: valida e calcula antes de descartar o ramo redo.
    /// Retorna Arc para evitar cópia adicional no app consumidor.
    pub fn aplicar(&mut self, operacao: OperacaoImagem) -> Result<Arc<ImagemRgba>, ErroPipeline> {
        let atual = self.resultado()?;
        let proximo = Arc::new(operacao.executar(&atual)?);

        self.operacoes.truncate(self.cursor);
        self.cache.retain(|(etapa, _)| *etapa <= self.cursor);
        self.cache_bytes = self.cache.iter().map(|(_, v)| v.pixels().len()).sum();
        self.operacoes.push(operacao);
        self.cursor += 1;
        self.guardar(self.cursor, Arc::clone(&proximo));
        Ok(proximo)
    }

    pub fn desfazer(&mut self) -> Result<Option<Arc<ImagemRgba>>, ErroPipeline> {
        if !self.pode_desfazer() { return Ok(None); }
        self.cursor -= 1;
        self.resultado().map(Some)
    }

    pub fn refazer(&mut self) -> Result<Option<Arc<ImagemRgba>>, ErroPipeline> {
        if !self.pode_refazer() { return Ok(None); }
        self.cursor += 1;
        self.resultado().map(Some)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn pixels() -> ImagemRgba {
        ImagemRgba::nova(
            2, 2,
            vec![
                255, 0, 0, 255,    0, 255, 0, 128,
                0, 0, 255, 0,      255, 255, 255, 64,
            ],
        ).unwrap()
    }

    #[test]
    fn pipeline_imutavel_preserva_original_e_versoes_anteriores() {
        let p0 = PipelineImagem::novo(pixels());
        let p1 = p0.com_operacao(OperacaoImagem::EspelharHorizontal);
        let p2 = p1.com_operacao(OperacaoImagem::EspelharVertical);
        assert_eq!(p0.operacoes().len(), 0);
        assert_eq!(p1.operacoes().len(), 1);
        assert_eq!(p2.operacoes().len(), 2);
        assert_eq!(p0.executar().unwrap(), pixels());
        assert_eq!(p1.executar().unwrap().pixels(), &[
            0, 255, 0, 128, 255, 0, 0, 255,
            255, 255, 255, 64, 0, 0, 255, 0,
        ]);
        assert_eq!(p2.executar().unwrap().dimensoes(), (2, 2));
        assert_eq!(p0.origem().pixels(), pixels().pixels());
    }

    #[test]
    fn undo_redo_e_ramificacao_nao_mutam_fonte() {
        let mut h = HistoricoImagem::novo(pixels());
        assert!(!h.pode_desfazer());
        let horizontal = h.aplicar(OperacaoImagem::EspelharHorizontal).unwrap();
        let finalizada = h.aplicar(OperacaoImagem::TonsDeCinza).unwrap();
        assert_eq!(h.indice(), 2);
        assert_ne!(horizontal, finalizada);
        let volta = h.desfazer().unwrap().unwrap();
        assert_eq!(volta, horizontal);
        assert_eq!(h.refazer().unwrap().unwrap(), finalizada);
        assert_eq!(h.desfazer().unwrap().unwrap(), horizontal);

        let novo_ramo = h.aplicar(OperacaoImagem::EspelharVertical).unwrap();
        assert_ne!(novo_ramo, finalizada);
        assert!(!h.pode_refazer());
        assert!(h.refazer().unwrap().is_none());
        assert_eq!(h.indice(), 2);
        assert_eq!(h.origem().pixels(), pixels().pixels());
        assert_eq!(h.pipeline_atual().executar().unwrap(), *novo_ramo);
    }

    #[test]
    fn erros_nao_descartam_redo_nem_alteram_indice() {
        let mut h = HistoricoImagem::novo(pixels());
        h.aplicar(OperacaoImagem::TonsDeCinza).unwrap();
        h.desfazer().unwrap();
        assert!(h.pode_refazer());
        let invalida = OperacaoImagem::AjustarLuz { brilho: f32::NAN, contraste: 0.0 };
        assert_eq!(h.aplicar(invalida), Err(ErroPipeline::ParametroInvalido));
        assert_eq!(h.indice(), 0);
        assert!(h.pode_refazer());
        assert_eq!(h.refazer().unwrap().unwrap(), Arc::new(
            OperacaoImagem::TonsDeCinza.executar(&pixels()).unwrap()
        ));
    }

    #[test]
    fn mascara_diferente_recusada_sem_efeitos() {
        let mascara = MascaraAlfa::nova(1, 1, vec![0]).unwrap();
        let mut h = HistoricoImagem::novo(pixels());
        assert_eq!(
            h.aplicar(OperacaoImagem::AplicarMascara(mascara)),
            Err(ErroPipeline::Mascara(ErroMascara::DimensoesDiferentes))
        );
        assert_eq!(h.operacoes().len(), 0);
        assert_eq!(h.origem(), &pixels());
    }

    #[test]
    fn redimensionamento_opacidade_e_undo_com_dimensoes_diferentes() {
        let mut h = HistoricoImagem::novo(pixels());
        let resized = h.aplicar(OperacaoImagem::Redimensionar {
            largura: 4, altura: 3, interpolacao: Some(Interpolacao::Bilinear),
        }).unwrap();
        assert_eq!(resized.dimensoes(), (4, 3));
        let transparente = h.aplicar(OperacaoImagem::Opacidade { fator: 0.0 }).unwrap();
        assert!(transparente.pixels().chunks_exact(4).all(|c| c[3] == 0));
        assert_eq!(h.desfazer().unwrap().unwrap(), resized);
        assert_eq!(h.desfazer().unwrap().unwrap().dimensoes(), (2, 2));
        assert_eq!(h.refazer().unwrap().unwrap(), resized);
        assert_eq!(h.pipeline_atual().executar().unwrap(), *resized);
    }

    #[test]
    fn cache_lru_limitado_e_desativavel_sem_perder_historico() {
        let mut h = HistoricoImagem::com_limite_cache(pixels(), 16);
        for _ in 0..12 {
            h.aplicar(OperacaoImagem::EspelharHorizontal).unwrap();
            assert!(h.bytes_cache() <= 16);
            assert!(h.quantidade_snapshots() <= 1);
        }
        let fim = h.resultado().unwrap();
        for _ in 0..12 { h.desfazer().unwrap(); }
        assert_eq!(h.resultado().unwrap(), Arc::new(pixels()));
        for _ in 0..12 { h.refazer().unwrap(); }
        assert_eq!(h.resultado().unwrap(), fim);
        h.configurar_limite_cache(0);
        assert_eq!(h.bytes_cache(), 0);
        assert_eq!(h.quantidade_snapshots(), 0);
        assert_eq!(h.resultado().unwrap(), fim);
    }

    #[test]
    fn recorte_produz_erro_atomico_e_operacoes_tem_ordem() {
        let mut h = HistoricoImagem::novo(pixels());
        h.aplicar(OperacaoImagem::Cortar {
            x: 0, y: 1, largura: 2, altura: 1,
        }).unwrap();
        assert_eq!(h.resultado().unwrap().pixels(), &[0, 0, 255, 0, 255, 255, 255, 64]);
        let antes = h.resultado().unwrap();
        assert_eq!(h.aplicar(OperacaoImagem::Cortar {
            x: 0, y: 1, largura: 2, altura: 1,
        }), Err(ErroPipeline::Imagem(ErroImagem::RecorteInvalido)));
        assert_eq!(h.indice(), 1);
        assert_eq!(h.resultado().unwrap(), antes);
    }

    #[test]
    fn filtros_complementares_e_erro_visual_tambem_reversiveis() {
        let mut h = HistoricoImagem::novo(pixels());
        h.aplicar(OperacaoImagem::AjustarCores(AjustesCor::default())).unwrap();
        h.aplicar(OperacaoImagem::ReduzirRuido { raio: 1 }).unwrap();
        h.aplicar(OperacaoImagem::Desfocar { raio: 1 }).unwrap();
        h.aplicar(OperacaoImagem::Nitidez { intensidade: 0.3 }).unwrap();
        assert_eq!(h.aplicar(OperacaoImagem::Perspectiva { intensidade: 2.0 }),
            Err(ErroPipeline::Visual(ErroVisual::ParametroInvalido)));
        assert_eq!(h.indice(), 4);
        h.aplicar(OperacaoImagem::Orientar(OrientacaoExif::Rotacao90)).unwrap();
        assert_eq!(h.resultado().unwrap().dimensoes(), (2, 2));
        assert_eq!(h.pipeline_atual().executar().unwrap(), *h.resultado().unwrap());
    }

    #[test]
    fn cache_zero_reconstroi_ao_refazer_sem_guardar_snapshot() {
        let mut h = HistoricoImagem::com_limite_cache(pixels(), 0);
        h.aplicar(OperacaoImagem::EspelharHorizontal).unwrap();
        let antes = h.resultado().unwrap();
        assert_eq!(h.bytes_cache(), 0);
        h.desfazer().unwrap();
        assert_eq!(h.refazer().unwrap().unwrap(), antes);
        assert_eq!(h.quantidade_snapshots(), 0);
    }
}
