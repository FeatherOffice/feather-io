//! Registro de codecs para importação/exportação por aplicativos.
//! Os formatos nativos (.fshz, .fsdz, .fdoz) mantêm seus codecs existentes;
//! adaptadores futuros os registram aqui sem acoplar os motores às GUIs.

pub mod csv_lotes;
pub mod deteccao;
pub mod exportadores;
pub mod ooxml;
pub mod formatos;
pub mod seguranca;
pub mod intermediario;
pub mod fluxo;
pub mod memoria;

use feather_charts::Grafico;
use feather_draw::Cena;
use feather_image::ImagemRgba;
use feather_tables::{Celula, Tabela, Valor};
use std::collections::HashMap;
use std::io::{Read, Write};

#[derive(Debug, Clone, PartialEq)]
pub enum Documento {
    Tabela(Tabela),
    Grafico(Grafico),
    Imagem(ImagemRgba),
    Desenho(Cena),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErroIo {
    FormatoDesconhecido(String),
    TipoIncompativel,
    DadosInvalidos(String),
    LimiteExcedido,
}
pub trait Codec: Send + Sync {
    fn extensoes(&self) -> &'static [&'static str];
    /// Ausência de declaração significa que ainda NÃO há suporte confirmado.
    /// Codecs reais devem declarar explicitamente os tipos de leitura/gravação.
    fn capacidades(&self) -> &'static [formatos::Capacidade] {
        &[]
    }
    fn ler(&self, bytes: &[u8]) -> Result<Documento, ErroIo>;
    fn gravar(&self, documento: &Documento) -> Result<Vec<u8>, ErroIo>;
    /// Opt-in: não recorre a `read_to_end` quando o codec não implementa streaming.
    fn ler_fluxo(
        &self, _leitor: &mut dyn Read, _limites: &fluxo::LimitesFluxo,
        _controle: &mut fluxo::ControleFluxo<'_>,
    ) -> Result<Documento, fluxo::ErroFluxo> {
        Err(fluxo::ErroFluxo::FluxoNaoSuportado)
    }
    /// Saída incremental: codecs sem suporte explícito recusam a operação.
    fn gravar_fluxo(
        &self, _documento: &Documento, _escritor: &mut dyn Write,
        _limites: &fluxo::LimitesFluxo, _controle: &mut fluxo::ControleFluxo<'_>,
    ) -> Result<(), fluxo::ErroFluxo> {
        Err(fluxo::ErroFluxo::FluxoNaoSuportado)
    }
}
#[derive(Default)]
pub struct Registro {
    codecs: HashMap<String, Box<dyn Codec>>,
}
impl Registro {
    pub fn novo() -> Self {
        Self::default()
    }
    pub fn padrao() -> Self {
        let mut registro = Self::novo();
        registro.registrar(Csv);
        // Formatos de exportação somente escrita (leitura não anunciada).
        registro.registrar(exportadores::Svg::default());
        registro.registrar(exportadores::Pdf::default());
        registro.registrar(exportadores::Png);
        registro.registrar(ooxml::Xlsx);
        registro.registrar(ooxml::Docx);
        registro.registrar(ooxml::Pptx);
        registro
    }
    pub fn registrar(&mut self, codec: impl Codec + 'static) {
        let extensoes = codec.extensoes();
        let mut entradas: Vec<String> = Vec::new();
        for ext in extensoes {
            let normalizada = ext.trim_start_matches('.').to_ascii_lowercase();
            if !normalizada.is_empty() {
                entradas.push(normalizada);
            }
        }
        if entradas.is_empty() {
            return;
        }
        // Compartilhar codec entre aliases sem duplicar a implementação.
        let codec: std::sync::Arc<dyn Codec> = std::sync::Arc::new(codec);
        for ext in entradas {
            self.codecs
                .insert(ext, Box::new(CodecCompartilhado(codec.clone())));
        }
    }
    pub fn ler(&self, extensao: &str, bytes: &[u8]) -> Result<Documento, ErroIo> {
        self.codec(extensao)?.ler(bytes)
    }
    pub fn gravar(&self, extensao: &str, doc: &Documento) -> Result<Vec<u8>, ErroIo> {
        self.codec(extensao)?.gravar(doc)
    }
    /// Entrada incremental sem materialização do arquivo inteiro.
    pub fn ler_fluxo(
        &self, extensao: &str, leitor: &mut dyn Read,
        limites: &fluxo::LimitesFluxo, controle: &mut fluxo::ControleFluxo<'_>,
    ) -> Result<Documento, fluxo::ErroFluxo> {
        self.codec(extensao).map_err(fluxo::ErroFluxo::from)?.ler_fluxo(leitor, limites, controle)
    }
    /// Escreve diretamente no destino; salvamento atômico fica a cargo do app (IO-012).
    pub fn gravar_fluxo(
        &self, extensao: &str, documento: &Documento, escritor: &mut dyn Write,
        limites: &fluxo::LimitesFluxo, controle: &mut fluxo::ControleFluxo<'_>,
    ) -> Result<(), fluxo::ErroFluxo> {
        self.codec(extensao).map_err(fluxo::ErroFluxo::from)?.gravar_fluxo(documento, escritor, limites, controle)
    }
    /// Catálogo efetivo: conhecidos sem codec são listados como NÃO suportados.
    /// A matriz se atualiza automaticamente quando codecs são registrados.
    pub fn catalogo(&self) -> formatos::Catalogo {
        let mut catalogo = formatos::Catalogo::novo();
        for (extensao, codec) in &self.codecs {
            catalogo.atualizar(extensao, codec.capacidades());
        }
        catalogo
    }
    /// Consulta a capacidade declarada sem tentar ler ou gravar dados.
    pub fn pode_converter(
        &self,
        origem: &str,
        destino: &str,
        tipo: formatos::TipoDocumento,
    ) -> bool {
        let catalogo = self.catalogo();
        catalogo.consultar(origem).is_some_and(|f| f.leitura(tipo))
            && catalogo
                .consultar(destino)
                .is_some_and(|f| f.gravacao(tipo))
    }
    /// Entrada com identificação segura do conteúdo; preserva `ler(extensão,...)`
    /// para os consumidores antigos que já controlam o formato externamente.
    /// ZIP é recusado enquanto não houver identificação por entradas internas.
    pub fn ler_identificado(&self, nome: &str, bytes: &[u8]) -> Result<Documento, ErroIo> {
        let formato = deteccao::identificar(nome, bytes)
            .map_err(|e| ErroIo::DadosInvalidos(format!("identificação do arquivo: {e:?}")))?;
        let catalogo = self.catalogo();
        if !catalogo
            .consultar(&formato)
            .is_some_and(|f| !f.pode_ler.is_empty())
        {
            return Err(ErroIo::FormatoDesconhecido(formato));
        }
        self.ler(&formato, bytes)
    }
    /// Conversão legada também bloqueia perdas: consumidores que precisam
    /// autorizá-las devem chamar `converter_com_politica` e exibir os avisos.
    pub fn converter(&self, origem: &str, destino: &str, bytes: &[u8]) -> Result<Vec<u8>, ErroIo> {
        use intermediario::{ErroConversao, PoliticaConversao};
        self.converter_com_politica(origem, destino, bytes, PoliticaConversao::BloquearPerdas)
            .map(|r| r.bytes)
            .map_err(|e| match e {
                ErroConversao::Io(io) => io,
                ErroConversao::PerdasDetectadas(avisos) => ErroIo::DadosInvalidos(format!(
                    "Conversão recusada por perda de atributos: {avisos:?}"
                )),
            })
    }
    fn codec(&self, ext: &str) -> Result<&dyn Codec, ErroIo> {
        let ext = ext.trim_start_matches('.').to_ascii_lowercase();
        self.codecs
            .get(&ext)
            .map(|c| c.as_ref())
            .ok_or(ErroIo::FormatoDesconhecido(ext))
    }
}
struct CodecCompartilhado(std::sync::Arc<dyn Codec>);
impl Codec for CodecCompartilhado {
    fn extensoes(&self) -> &'static [&'static str] {
        self.0.extensoes()
    }
    fn capacidades(&self) -> &'static [formatos::Capacidade] {
        self.0.capacidades()
    }
    fn ler(&self, bytes: &[u8]) -> Result<Documento, ErroIo> {
        self.0.ler(bytes)
    }
    fn gravar(&self, d: &Documento) -> Result<Vec<u8>, ErroIo> {
        self.0.gravar(d)
    }
    fn ler_fluxo(
        &self, leitor: &mut dyn Read, limites: &fluxo::LimitesFluxo,
        controle: &mut fluxo::ControleFluxo<'_>,
    ) -> Result<Documento, fluxo::ErroFluxo> {
        self.0.ler_fluxo(leitor, limites, controle)
    }
    fn gravar_fluxo(
        &self, documento: &Documento, escritor: &mut dyn Write,
        limites: &fluxo::LimitesFluxo, controle: &mut fluxo::ControleFluxo<'_>,
    ) -> Result<(), fluxo::ErroFluxo> {
        self.0.gravar_fluxo(documento, escritor, limites, controle)
    }
}

/// Codec CSV mínimo para troca tabular; o streaming de CSV muito grande
/// continua em feathersheet-io::csv até uma migração com benchmarks.
pub struct Csv;
const CAPACIDADES_CSV: [formatos::Capacidade; 1] = [formatos::Capacidade::nova(
    formatos::TipoDocumento::Tabela,
    true,
    true,
)];
impl Codec for Csv {
    fn ler_fluxo(
        &self, leitor: &mut dyn Read, limites: &fluxo::LimitesFluxo,
        controle: &mut fluxo::ControleFluxo<'_>,
    ) -> Result<Documento, fluxo::ErroFluxo> {
        fluxo::ler_csv(leitor, limites, controle)
    }
    fn gravar_fluxo(
        &self, documento: &Documento, escritor: &mut dyn Write,
        limites: &fluxo::LimitesFluxo, controle: &mut fluxo::ControleFluxo<'_>,
    ) -> Result<(), fluxo::ErroFluxo> {
        fluxo::gravar_csv(documento, escritor, limites, controle)
    }
    fn extensoes(&self) -> &'static [&'static str] {
        &["csv"]
    }
    fn capacidades(&self) -> &'static [formatos::Capacidade] {
        &CAPACIDADES_CSV
    }
    fn ler(&self, bytes: &[u8]) -> Result<Documento, ErroIo> {
        let conteudo = std::str::from_utf8(bytes)
            .map_err(|e| ErroIo::DadosInvalidos(e.to_string()))?
            .trim_start_matches('\u{feff}');
        let linhas = csv_lotes::dividir_csv(conteudo, ',');
        let colunas = linhas.iter().map(|l| l.len()).max().unwrap_or(0);
        let mut tabela = Tabela::nova(linhas.len(), colunas);
        for (r, linha) in linhas.into_iter().enumerate() {
            for (c, valor) in linha.into_iter().enumerate() {
                if !valor.is_empty() {
                    tabela
                        .definir(
                            r,
                            c,
                            Celula {
                                valor: Valor::Texto(valor),
                                ..Default::default()
                            },
                        )
                        .map_err(|e| ErroIo::DadosInvalidos(format!("{e:?}")))?;
                }
            }
        }
        Ok(Documento::Tabela(tabela))
    }
    fn gravar(&self, d: &Documento) -> Result<Vec<u8>, ErroIo> {
        let Documento::Tabela(tabela) = d else {
            return Err(ErroIo::TipoIncompativel);
        };
        let (linhas, colunas) = tabela.dimensoes();
        if linhas.checked_mul(colunas).map_or(true, |n| n > 10_000_000) {
            return Err(ErroIo::LimiteExcedido);
        }
        let mut resultado = Vec::new();
        for r in 0..linhas {
            if r > 0 {
                resultado.push(b'\n');
            }
            for c in 0..colunas {
                if c > 0 {
                    resultado.push(b',');
                }
                let valor = tabela
                    .obter(r, c)
                    .map(|c| c.valor.texto())
                    .unwrap_or_default();
                // O mesmo escape do FeatherSheet, sem criar strings
                // temporárias ou reimplementar a semântica de aspas.
                csv_lotes::escrever_campo(&mut resultado, &valor, ',')
                    .expect("escrita em Vec<u8> nao falha");
            }
        }
        Ok(resultado)
    }
}
#[cfg(test)]
mod testes {
    use super::*;
    #[test]
    fn csv_com_aspas_multilinha_faz_roundtrip() {
        let registro = Registro::padrao();
        let fonte = b"produto,valor\n\"um, dois\",\"linha 1\nlinha 2\"\n";
        let documento = registro.ler(".CSV", fonte).unwrap();
        let bytes = registro.gravar("csv", &documento).unwrap();
        assert_eq!(documento, registro.ler("csv", &bytes).unwrap());
        assert_eq!(registro.converter("csv", "csv", fonte).unwrap(), bytes);
    }
    #[test]
    fn recusa_converter_tipos_incompativeis() {
        let registro = Registro::padrao();
        let img = ImagemRgba::nova(1, 1, vec![0, 0, 0, 255]).unwrap();
        assert_eq!(
            registro.gravar("csv", &Documento::Imagem(img)),
            Err(ErroIo::TipoIncompativel)
        );
        // O .xlsx só tem gravação de tabela no registro comum (ver tests/ooxml.rs): ler dá erro.
        assert!(registro.ler("xlsx", b"").is_err());
    }
    #[test]
    fn matriz_representa_suporte_real_sem_confundir_planos() {
        use formatos::TipoDocumento as T;
        let registro = Registro::padrao();
        let matriz = registro.catalogo();
        let csv = matriz.consultar(".CSV").unwrap();
        assert!(csv.leitura(T::Tabela) && csv.gravacao(T::Tabela));
        assert!(!csv.gravacao(T::Imagem));
        // .xlsx/.docx/.pptx gravam tabela pelo registro comum (tests/ooxml.rs); os formatos
        // nativos de cada aplicativo continuam sem codec comum.
        for ext in ["fshz", "fsdz", "fdoz"] {
            let formato = matriz.consultar(ext).unwrap();
            assert!(formato.reconhecido);
            assert!(
                !formato.qualquer_capacidade(),
                "{ext} não tem codec comum registrado"
            );
        }
        assert!(registro.pode_converter("csv", "csv", T::Tabela));
        assert!(registro.pode_converter("csv", "docx", T::Tabela));
        assert!(!registro.pode_converter("csv", "fshz", T::Tabela));
    }
    #[test]
    fn leitura_identificada_aceita_csv_e_recusa_mascaras_falsas() {
        let registro = Registro::padrao();
        assert!(matches!(
            registro.ler_identificado("dados.csv", b"nome,valor\nA,1"),
            Ok(Documento::Tabela(_))
        ));
        let png = b"\x89PNG\r\n\x1a\nresto";
        assert!(matches!(
            registro.ler_identificado("planilha.csv", png),
            Err(ErroIo::DadosInvalidos(_))
        ));
        assert!(matches!(
            registro.ler_identificado("relatorio.docx", b"PK\x03\x04resto"),
            Err(ErroIo::DadosInvalidos(_))
        ));
        assert_eq!(
            registro.ler_identificado("imagem.png", png),
            Err(ErroIo::FormatoDesconhecido("png".into()))
        );
    }
}
