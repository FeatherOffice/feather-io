//! Conversões com `image::RgbaImage` sem realocar ou copiar pixels.
//! A decodificação (PNG/JPEG/etc.) continua nos adaptadores dos aplicativos.

use crate::{ErroImagem, ImagemRgba};
use image::{ImageBuffer, Rgba, RgbaImage};

impl ImagemRgba {
    /// Toma posse do buffer de `image` sem copiar seus bytes.
    ///
    /// O buffer da origem deve ter dimensões não nulas e RGBA válido.
    pub fn de_image(imagem: RgbaImage) -> Result<Self, ErroImagem> {
        let (largura, altura) = imagem.dimensions();
        Self::nova(largura, altura, imagem.into_raw())
    }

    /// Devolve o buffer para `image` sem copiar pixels.
    ///
    /// Uma `ImagemRgba` só pode ser criada com dimensões/buffer válidos;
    /// por isso, a construção abaixo não pode falhar.
    pub fn em_image(self) -> RgbaImage {
        RgbaImage::from_raw(self.largura, self.altura, self.pixels)
            .expect("buffer RGBA previamente validado")
    }

    /// Visão somente leitura da imagem na API `image`, sem alocação.
    ///
    /// A visão empresta o slice original e não pode sobreviver a `self`.
    /// Para editar com `image`, transfira a propriedade via `em_image`.
    pub fn como_image(&self) -> ImageBuffer<Rgba<u8>, &[u8]> {
        ImageBuffer::from_raw(self.largura, self.altura, self.pixels.as_slice())
            .expect("buffer RGBA previamente validado")
    }
}

impl TryFrom<RgbaImage> for ImagemRgba {
    type Error = ErroImagem;

    fn try_from(imagem: RgbaImage) -> Result<Self, Self::Error> {
        Self::de_image(imagem)
    }
}

impl From<ImagemRgba> for RgbaImage {
    fn from(imagem: ImagemRgba) -> Self {
        imagem.em_image()
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn transfere_buffer_sem_copia_em_ambas_as_direcoes() {
        let original = RgbaImage::from_raw(
            2,
            2,
            vec![255, 0, 0, 0, 0, 255, 0, 128, 0, 0, 255, 255, 19, 29, 39, 17],
        )
        .unwrap();
        let ptr = original.as_raw().as_ptr();
        let interna = ImagemRgba::de_image(original).unwrap();
        assert_eq!(interna.pixels().as_ptr(), ptr);
        assert_eq!(interna.dimensoes(), (2, 2));
        let externa: RgbaImage = interna.into();
        assert_eq!(externa.as_raw().as_ptr(), ptr);
        assert_eq!(externa.get_pixel(1, 0).0, [0, 255, 0, 128]);
    }

    #[test]
    fn visao_emprestada_compartilha_pixels_sem_copiar() {
        let interna = ImagemRgba::nova(2, 1, vec![1, 2, 3, 0, 4, 5, 6, 255]).unwrap();
        let ptr = interna.pixels().as_ptr();
        let visao = interna.como_image();
        assert_eq!(visao.as_raw().as_ptr(), ptr);
        assert_eq!(visao.get_pixel(0, 0).0, [1, 2, 3, 0]);
        assert_eq!(visao.get_pixel(1, 0).0, [4, 5, 6, 255]);
    }

    #[test]
    fn dimensoes_nulas_na_origem_sao_rejeitadas() {
        let vazia = RgbaImage::new(0, 1);
        assert_eq!(
            ImagemRgba::try_from(vazia),
            Err(ErroImagem::DimensoesInvalidas)
        );
        let vazia = RgbaImage::new(1, 0);
        assert_eq!(
            ImagemRgba::de_image(vazia),
            Err(ErroImagem::DimensoesInvalidas)
        );
    }

    #[test]
    fn ida_e_volta_preserva_alpha_e_operacoes() {
        let entrada =
            RgbaImage::from_raw(3, 1, vec![10, 20, 30, 0, 40, 50, 60, 128, 70, 80, 90, 255])
                .unwrap();
        let mut interna = ImagemRgba::de_image(entrada).unwrap();
        interna.espelhar_horizontal();
        let saida = interna.em_image();
        assert_eq!(
            saida.into_raw(),
            vec![70, 80, 90, 255, 40, 50, 60, 128, 10, 20, 30, 0,]
        );
    }
}
