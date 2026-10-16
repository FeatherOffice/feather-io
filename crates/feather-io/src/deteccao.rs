//! Inspeção conservadora de bytes antes de escolher um codec.
//! PNG/JPEG/WebP/PDF têm assinaturas inequívocas para classificação inicial.
//! ZIP é compartilhado por DOCX/XLSX/PPTX/formatos nativos; jamais distinguir
//! esses tipos só pela extensão. CSV não tem assinatura e exige UTF-8 válido.

use std::path::Path;

use crate::formatos::normalizar_extensao;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Evidencia {
    /// Assinatura binária ou início SVG reconhecido.
    Assinatura,
    /// Texto UTF-8 verificável, mas sem assinatura exclusiva (CSV).
    TextoSemAssinatura,
    /// Contêiner ZIP: conteúdo interno ainda não identificado.
    ZipAmbiguo,
    /// Nenhuma evidência suficiente para abrir automaticamente.
    Indeterminada,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deteccao {
    pub extensao_informada: Option<String>,
    /// Formato inferido do conteúdo, NÃO somente do nome.
    pub formato: Option<String>,
    pub evidencia: Evidencia,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErroDeteccao {
    /// O conteúdo confirma um formato diferente da extensão indicada.
    ExtensaoIncompativel {
        informada: String,
        assinatura: String,
    },
    /// Os bytes não permitem classificar o arquivo com segurança.
    FormatoIndeterminado,
    /// ZIP sem inspeção de entradas internas não identifica DOCX/XLSX/PPTX.
    ZipNaoIdentificado,
}

/// Normaliza nomes como `arquivo.CSV`, caminhos ou apenas `.csv`.
fn extensao_do_nome(nome: &str) -> Option<String> {
    let arquivo = Path::new(nome).file_name()?.to_str()?;
    let ext = if arquivo.starts_with('.') && !arquivo[1..].contains('.') {
        arquivo.trim_start_matches('.')
    } else {
        Path::new(arquivo).extension()?.to_str()?
    };
    let ext = normalizar_extensao(ext);
    if ext.is_empty() {
        None
    } else {
        Some(ext)
    }
}

fn extensoes_iguais(assinatura: &str, declarada: &str) -> bool {
    assinatura == declarada
        || (assinatura == "jpg" && declarada == "jpeg")
        || (assinatura == "jpeg" && declarada == "jpg")
}

/// Retorna a extensão associada a uma assinatura **inequívoca**.
/// Não confundir a assinatura PK/ZIP com formatos OOXML.
fn assinatura_forte(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some("png");
    }
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return Some("jpg");
    }
    if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some("webp");
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some("gif");
    }
    // BMP precisa ter cabeçalho BITMAPFILEHEADER completo e bytes reservados
    // zerados. Um texto começando por "BM" não constitui assinatura confiável.
    if bytes.len() >= 14 && bytes.starts_with(b"BM") && &bytes[6..10] == b"\0\0\0\0" {
        return Some("bmp");
    }
    if bytes.starts_with(b"%PDF-") {
        return Some("pdf");
    }

    // SVG é texto XML; inspecionar o início sem admitir HTML com <svg embutido.
    let texto = std::str::from_utf8(bytes).ok()?;
    let inicio = texto.trim_start_matches('\u{feff}').trim_start();
    let inicio = if inicio.starts_with("<?xml") {
        let (_, resto) = inicio.split_once("?>")?;
        resto.trim_start()
    } else {
        inicio
    };
    if inicio.starts_with("<svg")
        && inicio
            .as_bytes()
            .get(4)
            .is_some_and(|c| matches!(*c, b'>' | b' ' | b'\n' | b'\r' | b'\t'))
    {
        return Some("svg");
    }
    None
}

fn zip(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK\x03\x04")
        || bytes.starts_with(b"PK\x05\x06")
        || bytes.starts_with(b"PK\x07\x08")
}

fn extensao_zip(ext: &str) -> bool {
    matches!(
        ext,
        "zip"
            | "docx"
            | "xlsx"
            | "pptx"
            | "odt"
            | "ods"
            | "odp"
            | "fshz"
            | "fsdz"
            | "fdoz"
            | "fcdz"
    )
}

/// A extensão é uma pista a verificar, nunca prova da identidade do arquivo.
/// Para texto CSV, evita confundir bytes arbitrários ou binários com CSV.
pub fn detectar(nome: &str, bytes: &[u8]) -> Result<Deteccao, ErroDeteccao> {
    let ext = extensao_do_nome(nome);
    if let Some(tipo) = assinatura_forte(bytes) {
        if let Some(ref declarada) = ext {
            if !extensoes_iguais(tipo, declarada) {
                return Err(ErroDeteccao::ExtensaoIncompativel {
                    informada: declarada.clone(),
                    assinatura: tipo.into(),
                });
            }
        }
        return Ok(Deteccao {
            extensao_informada: ext,
            formato: Some(tipo.into()),
            evidencia: Evidencia::Assinatura,
        });
    }

    if zip(bytes) {
        if let Some(ref declarada) = ext {
            if !extensao_zip(declarada) {
                return Err(ErroDeteccao::ExtensaoIncompativel {
                    informada: declarada.clone(),
                    assinatura: "zip".into(),
                });
            }
        }
        return Ok(Deteccao {
            extensao_informada: ext,
            formato: None,
            evidencia: Evidencia::ZipAmbiguo,
        });
    }

    let texto = std::str::from_utf8(bytes).ok();
    let texto_valido = texto.is_some_and(|s| !s.contains('\0'));
    if ext.as_deref() == Some("csv") && texto_valido {
        return Ok(Deteccao {
            extensao_informada: ext,
            formato: Some("csv".into()),
            evidencia: Evidencia::TextoSemAssinatura,
        });
    }

    Ok(Deteccao {
        extensao_informada: ext,
        formato: None,
        evidencia: Evidencia::Indeterminada,
    })
}

/// Só seleciona um formato quando há evidência suficiente. Um ZIP é sempre
/// ambíguo até ser inspecionado por um codec especializado do aplicativo.
pub fn identificar(nome: &str, bytes: &[u8]) -> Result<String, ErroDeteccao> {
    let d = detectar(nome, bytes)?;
    match d.evidencia {
        Evidencia::ZipAmbiguo => Err(ErroDeteccao::ZipNaoIdentificado),
        Evidencia::Indeterminada => Err(ErroDeteccao::FormatoIndeterminado),
        _ => d.formato.ok_or(ErroDeteccao::FormatoIndeterminado),
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn png_assinatura_vence_extensao_falsa() {
        let dados = b"\x89PNG\r\n\x1a\nresto";
        assert_eq!(identificar("imagem.png", dados).unwrap(), "png");
        assert_eq!(
            identificar("imagem.csv", dados),
            Err(ErroDeteccao::ExtensaoIncompativel {
                informada: "csv".into(),
                assinatura: "png".into(),
            })
        );
    }

    #[test]
    fn jpg_e_jpeg_sao_aliases_validos() {
        assert_eq!(
            identificar("foto.JPEG", &[0xff, 0xd8, 0xff, 0xe0]).unwrap(),
            "jpg"
        );
    }

    #[test]
    fn zip_nao_se_passa_por_docx_xlsx_ou_fshz() {
        let bytes = b"PK\x03\x04conteudo";
        for nome in ["a.docx", "b.xlsx", "c.fshz", "d.zip"] {
            assert_eq!(
                identificar(nome, bytes),
                Err(ErroDeteccao::ZipNaoIdentificado)
            );
            let d = detectar(nome, bytes).unwrap();
            assert_eq!(d.evidencia, Evidencia::ZipAmbiguo);
            assert!(d.formato.is_none());
        }
        assert!(matches!(
            identificar("falso.png", bytes),
            Err(ErroDeteccao::ExtensaoIncompativel { .. })
        ));
    }

    #[test]
    fn texto_com_prefixo_bm_nao_e_falso_bmp() {
        assert_eq!(
            identificar("notas.csv", b"BM,valor\\ntexto,2").unwrap(),
            "csv"
        );
        let bmp = [b'B', b'M', 14, 0, 0, 0, 0, 0, 0, 0, 10, 0, 0, 0];
        assert_eq!(identificar("desenho.bmp", &bmp).unwrap(), "bmp");
    }

    #[test]
    fn csv_so_com_nome_e_bytes_utf8_validos() {
        assert_eq!(
            identificar("dados.CSV", b"coluna,valor\nA,2").unwrap(),
            "csv"
        );
        assert_eq!(
            identificar("dados.csv", b"\xff\0"),
            Err(ErroDeteccao::FormatoIndeterminado)
        );
        assert_eq!(
            identificar("dados.xlsx", b"coluna,valor\nA,2"),
            Err(ErroDeteccao::FormatoIndeterminado)
        );
        assert_eq!(
            identificar("dados", b"coluna,valor\nA,2"),
            Err(ErroDeteccao::FormatoIndeterminado)
        );
    }

    #[test]
    fn suporta_webp_pdf_e_svg_sem_confiar_na_extensao() {
        let mut webp = b"RIFF".to_vec();
        webp.extend([4, 0, 0, 0]);
        webp.extend(b"WEBP");
        assert_eq!(identificar("a.webp", &webp).unwrap(), "webp");
        assert_eq!(identificar("arquivo.pdf", b"%PDF-1.7\n").unwrap(), "pdf");
        assert_eq!(
            identificar("a.svg", b"<?xml version=\"1.0\"?>\n<svg xmlns=\"x\"/>").unwrap(),
            "svg"
        );
        assert!(matches!(
            identificar("a.csv", b"%PDF-1.7\n"),
            Err(ErroDeteccao::ExtensaoIncompativel { .. })
        ));
    }
}
