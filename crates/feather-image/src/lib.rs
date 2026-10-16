//! Filtros e recortes de imagens RGBA, sem dependência de Iced, PDF ou codecs.
//! PNG/JPEG/WebP são decodificados pelos adaptadores dos aplicativos.

/// Base64 RFC 4648 e `data:` URLs (uma implementação compartilhada entre Slide e Docs).
pub mod base64;

/// Ajustes cromáticos reutilizáveis, sem dependência de Iced.
pub mod cores;
/// Nitidez, blur e redução de ruído reutilizáveis.
pub mod efeitos;
/// Presets de efeitos não destrutivos compartilhados entre Docs e Sheet.
pub mod efeitos_compartilhados;
pub use efeitos_compartilhados::EfeitoRapido;
pub use cores::{aplicar_cores_rgba, AjustesCor, ErroCor};
pub use efeitos::{realcar_detalhes_rgba, ErroEfeito};

/// Interpolação bilinear/bicúbica, com antialiasing e canal alfa correto.
pub mod amostragem;
/// Leitura de metadados EXIF e aplicação não destrutiva das oito orientações.
pub mod orientacao;
pub use amostragem::Interpolacao;
pub use orientacao::{detectar_orientacao_exif, OrientacaoExif};

/// Recortes por máscara alfa, fundo conectado e transições suaves.
pub mod mascaras;
pub use mascaras::{ErroMascara, MascaraAlfa};

/// Efeitos visuais de camada (perspectiva, sombra, glow, reflexo e opacidade).
pub mod visuais;
pub use visuais::ErroVisual;

/// Edições declarativas RGBA e histórico undo/redo com cache LRU limitado.
pub mod pipeline;
pub use pipeline::{ErroPipeline, HistoricoImagem, OperacaoImagem, PipelineImagem};

mod adaptador_image;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErroImagem {
    DimensoesInvalidas,
    PixelsInvalidos,
    RecorteInvalido,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImagemRgba {
    largura: u32,
    altura: u32,
    pixels: Vec<u8>,
}
impl ImagemRgba {
    pub fn nova(largura: u32, altura: u32, pixels: Vec<u8>) -> Result<Self, ErroImagem> {
        if largura == 0 || altura == 0 {
            return Err(ErroImagem::DimensoesInvalidas);
        }
        let esperado = (largura as usize)
            .checked_mul(altura as usize)
            .and_then(|n| n.checked_mul(4))
            .ok_or(ErroImagem::DimensoesInvalidas)?;
        if pixels.len() != esperado {
            return Err(ErroImagem::PixelsInvalidos);
        }
        Ok(Self {
            largura,
            altura,
            pixels,
        })
    }
    pub fn dimensoes(&self) -> (u32, u32) {
        (self.largura, self.altura)
    }
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }
    pub fn em_pixels(self) -> Vec<u8> {
        self.pixels
    }

    pub fn cortar(&self, x: u32, y: u32, largura: u32, altura: u32) -> Result<Self, ErroImagem> {
        if largura == 0
            || altura == 0
            || x.checked_add(largura).map_or(true, |n| n > self.largura)
            || y.checked_add(altura).map_or(true, |n| n > self.altura)
        {
            return Err(ErroImagem::RecorteInvalido);
        }
        let mut pixels = Vec::with_capacity(largura as usize * altura as usize * 4);
        for linha in y..y + altura {
            let inicio = (linha as usize * self.largura as usize + x as usize) * 4;
            pixels.extend_from_slice(&self.pixels[inicio..inicio + largura as usize * 4]);
        }
        Self::nova(largura, altura, pixels)
    }

    pub fn espelhar_horizontal(&mut self) {
        let w = self.largura as usize;
        for linha in self.pixels.chunks_exact_mut(w * 4) {
            for x in 0..w / 2 {
                for canal in 0..4 {
                    linha.swap(x * 4 + canal, (w - 1 - x) * 4 + canal);
                }
            }
        }
    }
    pub fn espelhar_vertical(&mut self) {
        let stride = self.largura as usize * 4;
        for y in 0..(self.altura as usize / 2) {
            let oposta = self.altura as usize - 1 - y;
            for x in 0..stride {
                self.pixels.swap(y * stride + x, oposta * stride + x);
            }
        }
    }
    pub fn redimensionar_vizinho(&self, largura: u32, altura: u32) -> Result<Self, ErroImagem> {
        if largura == 0 || altura == 0 {
            return Err(ErroImagem::DimensoesInvalidas);
        }
        let len = (largura as usize)
            .checked_mul(altura as usize)
            .and_then(|v| v.checked_mul(4))
            .ok_or(ErroImagem::DimensoesInvalidas)?;
        let mut dados = vec![0u8; len];
        for y in 0..altura {
            let oy = (y as u64 * self.altura as u64 / altura as u64) as usize;
            for x in 0..largura {
                let ox = (x as u64 * self.largura as u64 / largura as u64) as usize;
                let a = (oy * self.largura as usize + ox) * 4;
                let b = (y as usize * largura as usize + x as usize) * 4;
                dados[b..b + 4].copy_from_slice(&self.pixels[a..a + 4]);
            }
        }
        Self::nova(largura, altura, dados)
    }
    /// Brilho e contraste em -1..1. Alfa preservado.
    pub fn ajustar_luz(&mut self, brilho: f32, contraste: f32) {
        let b = if brilho.is_finite() {
            brilho.clamp(-1.0, 1.0)
        } else {
            0.0
        };
        let c = if contraste.is_finite() {
            contraste.clamp(-1.0, 1.0)
        } else {
            0.0
        };
        for pixel in self.pixels.chunks_exact_mut(4) {
            for canal in &mut pixel[..3] {
                let x = f32::from(*canal) / 255.0;
                *canal = (((x - 0.5) * (1.0 + c) + 0.5 + b).clamp(0.0, 1.0) * 255.0).round() as u8;
            }
        }
    }
    pub fn tons_de_cinza(&mut self) {
        for p in self.pixels.chunks_exact_mut(4) {
            let y = (f32::from(p[0]) * 0.299 + f32::from(p[1]) * 0.587 + f32::from(p[2]) * 0.114)
                .round() as u8;
            p[0] = y;
            p[1] = y;
            p[2] = y;
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    #[test]
    fn preserva_transparencia_em_todos_os_filtros() {
        let mut im = ImagemRgba::nova(2, 1, vec![255, 0, 0, 90, 0, 0, 255, 120]).unwrap();
        im.espelhar_horizontal();
        im.tons_de_cinza();
        im.ajustar_luz(0.1, 0.2);
        assert_eq!((im.pixels()[3], im.pixels()[7]), (120, 90));
        assert_eq!(im.redimensionar_vizinho(4, 2).unwrap().dimensoes(), (4, 2));
    }
    #[test]
    fn recorte_e_tamanho_verificados() {
        let im = ImagemRgba::nova(1, 1, vec![1, 2, 3, 4]).unwrap();
        assert_eq!(im.cortar(1, 0, 1, 1), Err(ErroImagem::RecorteInvalido));
        assert_eq!(im.cortar(0, 0, 1, 1).unwrap().pixels(), &[1, 2, 3, 4]);
        assert_eq!(
            ImagemRgba::nova(0, 1, vec![]),
            Err(ErroImagem::DimensoesInvalidas)
        );
    }
}
