//! Orientação EXIF de imagens JPEG, TIFF, PNG(eXIf) e WebP(EXIF).
//! Somente metadados são lidos aqui: a decodificação continua no aplicativo.
//! Os bytes originais nunca são modificados. Limites são checados antes de ler.

use crate::{ErroImagem, ImagemRgba};

/// Valores 1..8 definidos no TIFF/EXIF (tag 0x0112).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum OrientacaoExif {
    Normal = 1,
    EspelhoHorizontal = 2,
    Rotacao180 = 3,
    EspelhoVertical = 4,
    Transposta = 5,
    Rotacao90 = 6,
    Transversa = 7,
    Rotacao270 = 8,
}

impl OrientacaoExif {
    pub fn de_valor(numero: u16) -> Option<Self> {
        match numero {
            1 => Some(Self::Normal),
            2 => Some(Self::EspelhoHorizontal),
            3 => Some(Self::Rotacao180),
            4 => Some(Self::EspelhoVertical),
            5 => Some(Self::Transposta),
            6 => Some(Self::Rotacao90),
            7 => Some(Self::Transversa),
            8 => Some(Self::Rotacao270),
            _ => None,
        }
    }

    pub fn troca_eixos(self) -> bool {
        matches!(
            self,
            Self::Transposta | Self::Rotacao90 | Self::Transversa | Self::Rotacao270
        )
    }
}

fn u16_tiff(bytes: &[u8], pos: usize, little: bool) -> Option<u16> {
    let src: [u8; 2] = bytes.get(pos..pos.checked_add(2)?)?.try_into().ok()?;
    Some(if little {
        u16::from_le_bytes(src)
    } else {
        u16::from_be_bytes(src)
    })
}

fn u32_tiff(bytes: &[u8], pos: usize, little: bool) -> Option<u32> {
    let src: [u8; 4] = bytes.get(pos..pos.checked_add(4)?)?.try_into().ok()?;
    Some(if little {
        u32::from_le_bytes(src)
    } else {
        u32::from_be_bytes(src)
    })
}

/// Localiza a orientação no IFD0 do TIFF/EXIF, com offsets relativos ao TIFF.
fn orientacao_tiff(tiff: &[u8]) -> Option<OrientacaoExif> {
    let little = match tiff.get(..2)? {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    if u16_tiff(tiff, 2, little)? != 42 {
        return None;
    }
    let inicio = usize::try_from(u32_tiff(tiff, 4, little)?).ok()?;
    if inicio < 8 {
        return None;
    }
    let quantidade = usize::from(u16_tiff(tiff, inicio, little)?);
    // Cada entrada ocupa 12 bytes; recusar IFD truncado ou offset hostil.
    let comeco = inicio.checked_add(2)?;
    let fim = comeco.checked_add(quantidade.checked_mul(12)?)?;
    tiff.get(comeco..fim)?;
    for i in 0..quantidade {
        let entrada = comeco + i * 12;
        if u16_tiff(tiff, entrada, little)? != 0x0112 {
            continue;
        }
        if u16_tiff(tiff, entrada + 2, little)? != 3 {
            return None; // EXIF Orientation deve ser SHORT (2 bytes).
        }
        let count = u32_tiff(tiff, entrada + 4, little)?;
        if count == 0 || count > 1024 {
            return None;
        }
        let onde = if count <= 2 {
            entrada + 8 // O valor cabe nos quatro bytes da entrada.
        } else {
            usize::try_from(u32_tiff(tiff, entrada + 8, little)?).ok()?
        };
        return OrientacaoExif::de_valor(u16_tiff(tiff, onde, little)?);
    }
    None
}

fn exif_payload(payload: &[u8]) -> Option<OrientacaoExif> {
    if let Some(tiff) = payload.strip_prefix(b"Exif\0\0") {
        orientacao_tiff(tiff)
    } else {
        orientacao_tiff(payload)
    }
}

fn exif_jpeg(bytes: &[u8]) -> Option<OrientacaoExif> {
    let mut at = 2usize;
    while at < bytes.len() {
        if *bytes.get(at)? != 0xff {
            return None;
        }
        // JPEG permite vários 0xff consecutivos antes do marcador.
        while bytes.get(at) == Some(&0xff) {
            at = at.checked_add(1)?;
        }
        let marker = *bytes.get(at)?;
        at = at.checked_add(1)?;
        if marker == 0xda || marker == 0xd9 {
            return None; // SOS/EOI: nunca pesquisar dentro da imagem comprimida.
        }
        if marker == 0xd8 || marker == 0x01 || (0xd0..=0xd7).contains(&marker) {
            continue;
        }
        let len = usize::from(u16::from_be_bytes(bytes.get(at..at + 2)?.try_into().ok()?));
        if len < 2 {
            return None;
        }
        let fim = at.checked_add(len)?;
        let segmento = bytes.get(at + 2..fim)?;
        if marker == 0xe1 {
            if let Some(orientacao) = exif_payload(segmento) {
                return Some(orientacao);
            }
        }
        at = fim;
    }
    None
}

fn exif_png(bytes: &[u8]) -> Option<OrientacaoExif> {
    let mut at = 8usize;
    while at < bytes.len() {
        let header = bytes.get(at..at.checked_add(8)?)?;
        let len = u32::from_be_bytes(header[..4].try_into().ok()?) as usize;
        let fim_dados = at.checked_add(8)?.checked_add(len)?;
        let dados = bytes.get(at + 8..fim_dados)?;
        // O chunk possui CRC de quatro bytes, mesmo quando não é verificado aqui.
        bytes.get(fim_dados..fim_dados.checked_add(4)?)?;
        if &header[4..8] == b"eXIf" {
            return exif_payload(dados);
        }
        if &header[4..8] == b"IEND" {
            break;
        }
        at = fim_dados + 4;
    }
    None
}

fn exif_webp(bytes: &[u8]) -> Option<OrientacaoExif> {
    let declarados = u32::from_le_bytes(bytes.get(4..8)?.try_into().ok()?) as usize;
    let limite = declarados.checked_add(8)?;
    if limite > bytes.len() || limite < 12 {
        return None;
    }
    let mut at = 12usize;
    while at < limite {
        let header = bytes.get(at..at.checked_add(8)?)?;
        let len = u32::from_le_bytes(header[4..8].try_into().ok()?) as usize;
        let fim = at.checked_add(8)?.checked_add(len)?;
        let payload = bytes.get(at + 8..fim)?;
        if fim > limite {
            return None;
        }
        if &header[..4] == b"EXIF" {
            return exif_payload(payload);
        }
        // RIFF usa padding até o próximo byte par.
        at = fim.checked_add(len & 1)?;
    }
    None
}

/// Extrai a tag EXIF 0x0112 de JPEG, TIFF, PNG/eXIf e WebP/EXIF.
/// Retorna None quando ausente, desconhecida ou malformada; não faz I/O.
/// Não valida checksum/CRC ou autenticidade do arquivo (função só de metadados).
pub fn detectar_orientacao_exif(bytes: &[u8]) -> Option<OrientacaoExif> {
    if bytes.starts_with(b"\xff\xd8") {
        exif_jpeg(bytes)
    } else if bytes.starts_with(b"II") || bytes.starts_with(b"MM") {
        orientacao_tiff(bytes)
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        exif_png(bytes)
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12).is_some_and(|fourcc| fourcc == b"WEBP")
    {
        exif_webp(bytes)
    } else {
        None
    }
}

impl ImagemRgba {
    /// Cria imagem com orientação corrigida; não altera a origem.
    /// Para EXIF 5..8 troca largura/altura; 1 faz clone exato.
    pub fn orientar_exif(&self, orientacao: OrientacaoExif) -> Result<Self, ErroImagem> {
        if orientacao == OrientacaoExif::Normal {
            return Ok(self.clone());
        }
        let (w, h) = (self.largura, self.altura);
        let (dest_w, dest_h) = if orientacao.troca_eixos() {
            (h, w)
        } else {
            (w, h)
        };
        let mut dados = Vec::new();
        dados
            .try_reserve_exact(self.pixels.len())
            .map_err(|_| ErroImagem::DimensoesInvalidas)?;
        dados.resize(self.pixels.len(), 0);
        for sy in 0..h {
            for sx in 0..w {
                let (dx, dy) = match orientacao {
                    OrientacaoExif::Normal => (sx, sy),
                    OrientacaoExif::EspelhoHorizontal => (w - 1 - sx, sy),
                    OrientacaoExif::Rotacao180 => (w - 1 - sx, h - 1 - sy),
                    OrientacaoExif::EspelhoVertical => (sx, h - 1 - sy),
                    OrientacaoExif::Transposta => (sy, sx),
                    OrientacaoExif::Rotacao90 => (h - 1 - sy, sx),
                    OrientacaoExif::Transversa => (h - 1 - sy, w - 1 - sx),
                    OrientacaoExif::Rotacao270 => (sy, w - 1 - sx),
                };
                let src = (sy as usize * w as usize + sx as usize) * 4;
                let dst = (dy as usize * dest_w as usize + dx as usize) * 4;
                dados[dst..dst + 4].copy_from_slice(&self.pixels[src..src + 4]);
            }
        }
        Self::nova(dest_w, dest_h, dados)
    }

    /// Usa a orientação dos bytes codificados; sem EXIF, devolve clone fiel.
    /// Nenhuma alteração é aplicada ao JPEG/PNG/WebP original.
    pub fn corrigir_orientacao_exif(&self, arquivo: &[u8]) -> Result<Self, ErroImagem> {
        self.orientar_exif(detectar_orientacao_exif(arquivo).unwrap_or(OrientacaoExif::Normal))
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn tiff(valor: u16, little: bool) -> Vec<u8> {
        let mut bytes = if little {
            b"II".to_vec()
        } else {
            b"MM".to_vec()
        };
        let short = |v: u16| -> [u8; 2] {
            if little {
                v.to_le_bytes()
            } else {
                v.to_be_bytes()
            }
        };
        let long = |v: u32| -> [u8; 4] {
            if little {
                v.to_le_bytes()
            } else {
                v.to_be_bytes()
            }
        };
        bytes.extend_from_slice(&short(42));
        bytes.extend_from_slice(&long(8)); // offset IFD0
        bytes.extend_from_slice(&short(1)); // 1 tag
        bytes.extend_from_slice(&short(0x0112)); // orientação
        bytes.extend_from_slice(&short(3)); // SHORT
        bytes.extend_from_slice(&long(1)); // 1 valor
        bytes.extend_from_slice(&short(valor));
        bytes.extend_from_slice(&[0, 0]);
        bytes.extend_from_slice(&long(0));
        bytes
    }

    fn imagem() -> ImagemRgba {
        let mut pixels = vec![];
        for i in 1..=6u8 {
            pixels.extend([i, i + 10, i + 20, i * 30]);
        }
        ImagemRgba::nova(2, 3, pixels).unwrap()
    }

    fn rótulos(imagem: &ImagemRgba) -> Vec<u8> {
        imagem.pixels().chunks_exact(4).map(|p| p[0]).collect()
    }

    #[test]
    fn oito_orientacoes_exif_sao_corretas() {
        let original = imagem();
        for (codigo, esperado, dimensoes) in [
            (1, vec![1, 2, 3, 4, 5, 6], (2, 3)),
            (2, vec![2, 1, 4, 3, 6, 5], (2, 3)),
            (3, vec![6, 5, 4, 3, 2, 1], (2, 3)),
            (4, vec![5, 6, 3, 4, 1, 2], (2, 3)),
            (5, vec![1, 3, 5, 2, 4, 6], (3, 2)),
            (6, vec![5, 3, 1, 6, 4, 2], (3, 2)),
            (7, vec![6, 4, 2, 5, 3, 1], (3, 2)),
            (8, vec![2, 4, 6, 1, 3, 5], (3, 2)),
        ] {
            let tag = OrientacaoExif::de_valor(codigo).unwrap();
            let saida = original.orientar_exif(tag).unwrap();
            assert_eq!(saida.dimensoes(), dimensoes, "EXIF {codigo}");
            assert_eq!(rótulos(&saida), esperado, "EXIF {codigo}");
            assert_eq!(rótulos(&original), vec![1, 2, 3, 4, 5, 6]);
            assert_eq!(saida.pixels().len(), original.pixels().len());
        }
    }

    #[test]
    fn tiff_big_e_little_endian_e_jpeg_exif() {
        for little in [true, false] {
            let t = tiff(6, little);
            assert_eq!(
                detectar_orientacao_exif(&t),
                Some(OrientacaoExif::Rotacao90)
            );
            let mut exif = b"Exif\0\0".to_vec();
            exif.extend(&t);
            let len = (exif.len() + 2) as u16;
            let mut jpeg = vec![0xff, 0xd8, 0xff, 0xe1];
            jpeg.extend_from_slice(&len.to_be_bytes());
            jpeg.extend_from_slice(&exif);
            jpeg.extend_from_slice(&[0xff, 0xd9]);
            let jpeg_original = jpeg.clone();
            assert_eq!(
                detectar_orientacao_exif(&jpeg),
                Some(OrientacaoExif::Rotacao90)
            );
            assert_eq!(
                imagem()
                    .corrigir_orientacao_exif(&jpeg)
                    .unwrap()
                    .dimensoes(),
                (3, 2)
            );
            assert_eq!(jpeg, jpeg_original);
        }
    }

    #[test]
    fn png_exif_e_webp_exif_sao_lidos_sem_decodificador() {
        let t = tiff(8, true);
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        png.extend_from_slice(&(t.len() as u32).to_be_bytes());
        png.extend_from_slice(b"eXIf");
        png.extend_from_slice(&t);
        png.extend_from_slice(&[0; 4]); // CRC fictício: parser não autentica contêiner.
        assert_eq!(
            detectar_orientacao_exif(&png),
            Some(OrientacaoExif::Rotacao270)
        );
        let tam_riff = (4 + 8 + t.len() + (t.len() & 1)) as u32;
        let mut webp = b"RIFF".to_vec();
        webp.extend_from_slice(&tam_riff.to_le_bytes());
        webp.extend_from_slice(b"WEBP");
        webp.extend_from_slice(b"EXIF");
        webp.extend_from_slice(&(t.len() as u32).to_le_bytes());
        webp.extend_from_slice(&t);
        if t.len() % 2 != 0 {
            webp.push(0);
        }
        assert_eq!(
            detectar_orientacao_exif(&webp),
            Some(OrientacaoExif::Rotacao270)
        );
    }

    #[test]
    fn metadados_invalidos_nao_causam_panic_nem_mutacao() {
        assert_eq!(detectar_orientacao_exif(&[]), None);
        assert_eq!(
            detectar_orientacao_exif(&[0xff, 0xd8, 0xff, 0xe1, 0xff]),
            None
        );
        assert_eq!(detectar_orientacao_exif(&tiff(9, true)), None);
        assert_eq!(detectar_orientacao_exif(&tiff(2, false)[..9]), None);
        assert_eq!(OrientacaoExif::de_valor(0), None);
        let im = imagem();
        assert_eq!(im.corrigir_orientacao_exif(b"sem metadados").unwrap(), im);
        assert_eq!(im.orientar_exif(OrientacaoExif::Normal).unwrap(), im);
    }
}
