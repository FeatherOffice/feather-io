//! Exportadores opt-in no catálogo IO; usam motores de domínio existentes.
//!
//! SVG/PDF preservam geometria vetorial de `Cena`. PNG preserva pixels RGBA
//! já processados pelo Image. Não é importador nem rasterizador de gráficos.
//! Leitura de saída exportada pertence a codecs consumidores específicos.
use crate::{formatos::{Capacidade, TipoDocumento}, Codec, Documento, ErroIo};
use image::{codecs::png::PngEncoder, ColorType, ImageEncoder};

/// Limites defensivos da exportação em memória. Saídas maiores precisam
/// de um adaptador específico com gravação incremental.
const MAX_SAIDA: usize = 128 * 1024 * 1024;
const MAX_PIXELS_PNG: u64 = 16_777_216;

/// Dimensões padrão de gráficos exportados (a Cena original usa suas próprias
/// dimensões). PDF interpreta as unidades da cena em pontos tipográficos.
pub const GRAFICO_LARGURA_PADRAO: f32 = 960.0;
pub const GRAFICO_ALTURA_PADRAO: f32 = 540.0;

const CAP_VETORES: [Capacidade; 2] = [
    Capacidade::nova(TipoDocumento::Desenho, false, true),
    Capacidade::nova(TipoDocumento::Grafico, false, true),
];
const CAP_PNG: [Capacidade; 1] = [
    Capacidade::nova(TipoDocumento::Imagem, false, true),
];

fn verificar_saida(bytes: Vec<u8>) -> Result<Vec<u8>, ErroIo> {
    if bytes.len() > MAX_SAIDA {
        Err(ErroIo::LimiteExcedido)
    } else {
        Ok(bytes)
    }
}

/// Codec de saída SVG 1.1: Cena e gráfico via Drawing, sem rasterização.
#[derive(Debug, Clone, Copy)]
pub struct Svg {
    pub largura_grafico: f32,
    pub altura_grafico: f32,
}
impl Default for Svg {
    fn default() -> Self {
        Self {
            largura_grafico: GRAFICO_LARGURA_PADRAO,
            altura_grafico: GRAFICO_ALTURA_PADRAO,
        }
    }
}
impl Codec for Svg {
    fn extensoes(&self) -> &'static [&'static str] { &["svg"] }
    fn capacidades(&self) -> &'static [Capacidade] { &CAP_VETORES }
    fn ler(&self, _bytes: &[u8]) -> Result<Documento, ErroIo> {
        Err(ErroIo::TipoIncompativel)
    }
    fn gravar(&self, documento: &Documento) -> Result<Vec<u8>, ErroIo> {
        let svg = match documento {
            Documento::Desenho(cena) => feather_draw::exportar_svg(cena)
                .map_err(|e| ErroIo::DadosInvalidos(format!("erro ao gerar SVG: {e:?}")))?,
            Documento::Grafico(grafico) => grafico
                .exportar_svg(self.largura_grafico, self.altura_grafico)
                .map_err(|e| ErroIo::DadosInvalidos(format!("erro ao gerar SVG do gráfico: {e:?}")))?,
            _ => return Err(ErroIo::TipoIncompativel),
        };
        verificar_saida(svg.into_bytes())
    }
}

/// Codec de saída PDF vetorial 1.4: Cena e gráfico via Drawing.
#[derive(Debug, Clone, Copy)]
pub struct Pdf {
    pub largura_grafico: f32,
    pub altura_grafico: f32,
}
impl Default for Pdf {
    fn default() -> Self {
        Self {
            largura_grafico: GRAFICO_LARGURA_PADRAO,
            altura_grafico: GRAFICO_ALTURA_PADRAO,
        }
    }
}
impl Codec for Pdf {
    fn extensoes(&self) -> &'static [&'static str] { &["pdf"] }
    fn capacidades(&self) -> &'static [Capacidade] { &CAP_VETORES }
    fn ler(&self, _bytes: &[u8]) -> Result<Documento, ErroIo> {
        Err(ErroIo::TipoIncompativel)
    }
    fn gravar(&self, documento: &Documento) -> Result<Vec<u8>, ErroIo> {
        let pdf = match documento {
            Documento::Desenho(cena) => feather_draw::exportar_pdf(cena)
                .map_err(|e| ErroIo::DadosInvalidos(format!("erro ao gerar PDF: {e:?}")))?,
            Documento::Grafico(grafico) => grafico
                .exportar_pdf(self.largura_grafico, self.altura_grafico)
                .map_err(|e| ErroIo::DadosInvalidos(format!("erro ao gerar PDF do gráfico: {e:?}")))?,
            _ => return Err(ErroIo::TipoIncompativel),
        };
        verificar_saida(pdf)
    }
}

/// Codec de saída PNG RGBA: preserva pixels e transparência de ImagemRgba,
/// sem rasterizar Cena/Grafico e sem copiar a matriz de pixels na entrada.
#[derive(Debug, Clone, Copy, Default)]
pub struct Png;
impl Codec for Png {
    fn extensoes(&self) -> &'static [&'static str] { &["png"] }
    fn capacidades(&self) -> &'static [Capacidade] { &CAP_PNG }
    fn ler(&self, _bytes: &[u8]) -> Result<Documento, ErroIo> {
        Err(ErroIo::TipoIncompativel)
    }
    fn gravar(&self, documento: &Documento) -> Result<Vec<u8>, ErroIo> {
        let Documento::Imagem(imagem) = documento else {
            return Err(ErroIo::TipoIncompativel);
        };
        let (largura, altura) = imagem.dimensoes();
        if u64::from(largura) * u64::from(altura) > MAX_PIXELS_PNG {
            return Err(ErroIo::LimiteExcedido);
        }
        let mut saida = Vec::new();
        PngEncoder::new(&mut saida)
            .write_image(imagem.pixels(), largura, altura, ColorType::Rgba8.into())
            .map_err(|e| ErroIo::DadosInvalidos(format!("erro ao gerar PNG: {e}")))?;
        verificar_saida(saida)
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::{formatos::TipoDocumento as TipoDoc, intermediario::{ErroConversao, PoliticaConversao}, Registro};
    use feather_charts::{Grafico, Serie, Tipo};
    use feather_draw::{Cena, Cor, Estilo, Figura, Retangulo};
    use feather_image::ImagemRgba;

    fn desenho() -> Documento {
        let mut cena = Cena::nova(90.0, 40.0);
        cena.adicionar(Figura::Retangulo {
            area: Retangulo::novo(5.0, 10.0, 20.0, 10.0),
            estilo: Estilo::default(),
        });
        Documento::Desenho(cena)
    }
    fn grafico() -> Documento {
        Documento::Grafico(Grafico {
            titulo: "Vendas".to_owned(),
            tipo: Tipo::Colunas,
            categorias: vec!["A".into(), "B".into()],
            series: vec![Serie {
                nome: "Qtde".into(),
                valores: vec![-3.0, 5.0],
                cor: Cor::rgb(10, 20, 30),
            }],
        })
    }

    #[test]
    fn capacidades_reais_sao_apenas_de_gravacao() {
        let reg = Registro::padrao();
        for (formato, tipos) in [
            ("svg", &[TipoDoc::Desenho, TipoDoc::Grafico][..]),
            ("pdf", &[TipoDoc::Desenho, TipoDoc::Grafico][..]),
            ("png", &[TipoDoc::Imagem][..]),
        ] {
            let info = reg.catalogo().consultar(formato).unwrap().clone();
            assert!(info.codec_registrado);
            // O catálogo ordena TipoDocumento por Ord; compare as
            // capacidades exatas independentemente da ordem de declaração.
            let mut esperados = tipos.to_vec();
            esperados.sort_unstable();
            assert_eq!(info.pode_gravar, esperados);
            assert!(info.pode_ler.is_empty());
            assert!(!reg.pode_converter("csv", formato, TipoDoc::Tabela));
            assert_eq!(reg.ler(formato, b"nada"), Err(ErroIo::TipoIncompativel));
        }
    }

    #[test]
    fn svg_vetorial_deterministico_sem_perda_de_unidades() {
        let reg = Registro::padrao();
        let a = reg.gravar(".SVG", &desenho()).unwrap();
        let texto = std::str::from_utf8(&a).unwrap();
        assert!(texto.starts_with("<svg "));
        assert!(texto.contains("viewBox=\"0 0 90 40\""));
        assert!(texto.contains("M 5 10 L 25 10 L 25 20 L 5 20 Z"));
        assert_eq!(a, reg.gravar("svg", &desenho()).unwrap());
    }

    #[test]
    fn pdf_vetorial_deterministico_preserva_coordenadas() {
        let reg = Registro::padrao();
        let a = reg.gravar("pdf", &desenho()).unwrap();
        let texto = String::from_utf8_lossy(&a);
        assert!(texto.starts_with("%PDF-1.4\n"));
        assert!(texto.contains("/MediaBox [0 0 90 40]"));
        assert!(texto.contains("5 10 m\n25 10 l"));
        assert_eq!(a, reg.gravar(".PDF", &desenho()).unwrap());
    }

    #[test]
    fn png_real_preserva_pixel_e_alfa_sem_rasterizar() {
        let reg = Registro::padrao();
        let original = vec![255, 0, 0, 0, 10, 20, 30, 127, 0, 0, 255, 255];
        let imagem = Documento::Imagem(ImagemRgba::nova(3, 1, original.clone()).unwrap());
        let bytes = reg.gravar("png", &imagem).unwrap();
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        let decodificada = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
            .unwrap()
            .to_rgba8();
        assert_eq!(decodificada.dimensions(), (3, 1));
        assert_eq!(decodificada.into_raw(), original);
    }

    #[test]
    fn grafico_exporta_mesma_cena_em_svg_e_pdf() {
        let reg = Registro::padrao();
        let imagem = grafico();
        let svg = reg.gravar("svg", &imagem).unwrap();
        let texto = std::str::from_utf8(&svg).unwrap();
        assert!(texto.contains("viewBox=\"0 0 960 540\""));
        let pdf = reg.gravar("pdf", &imagem).unwrap();
        assert!(pdf.starts_with(b"%PDF-1.4\n"));
        assert!(String::from_utf8_lossy(&pdf).contains("/MediaBox [0 0 960 540]"));
    }

    #[test]
    fn pizza_preserva_rotulos_no_svg_e_no_pdf_do_charts() {
        let pizza = Documento::Grafico(Grafico {
            titulo: String::new(),
            tipo: Tipo::Pizza,
            categorias: vec!["Alpha".into(), "Beta".into()],
            series: vec![Serie {
                nome: "Partes".into(),
                valores: vec![4.0, 6.0],
                cor: Cor::rgb(11, 23, 45),
            }],
        });
        let reg = Registro::padrao();
        let svg = reg.gravar("svg", &pizza).unwrap();
        assert!(std::str::from_utf8(&svg).unwrap().contains("<text "));
        let pdf = reg.gravar("pdf", &pizza).unwrap();
        assert!(String::from_utf8_lossy(&pdf).contains("BT /F1"));
    }

    #[test]
    fn entradas_indevidas_e_graficos_invalidos_nao_geram_arquivos() {
        let reg = Registro::padrao();
        let img = Documento::Imagem(ImagemRgba::nova(1, 1, vec![1, 2, 3, 4]).unwrap());
        assert_eq!(reg.gravar("pdf", &img), Err(ErroIo::TipoIncompativel));
        assert_eq!(reg.gravar("svg", &img), Err(ErroIo::TipoIncompativel));
        assert_eq!(reg.gravar("png", &desenho()), Err(ErroIo::TipoIncompativel));
        let invalido = Documento::Grafico(Grafico {
            titulo: String::new(), tipo: Tipo::Linhas,
            categorias: vec!["X".into()], series: Vec::new(),
        });
        assert!(matches!(reg.gravar("pdf", &invalido), Err(ErroIo::DadosInvalidos(_))));
        assert!(matches!(reg.gravar("svg", &invalido), Err(ErroIo::DadosInvalidos(_))));
    }

    #[test]
    fn politica_segura_nao_afirma_roundtrip_em_codec_apenas_saida() {
        let reg = Registro::padrao();
        assert!(matches!(
            reg.gravar_com_politica("png", &Documento::Imagem(
                ImagemRgba::nova(1, 1, vec![0, 0, 0, 255]).unwrap()),
                PoliticaConversao::BloquearPerdas),
            Err(ErroConversao::PerdasDetectadas(_))
        ));
        let saida = reg.gravar_com_politica(
            "svg", &desenho(), PoliticaConversao::PermitirComAvisos).unwrap();
        assert!(!saida.sem_perdas_verificadas());
        assert!(saida.bytes.starts_with(b"<svg"));
    }
}
