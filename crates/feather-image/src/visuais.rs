//! Efeitos de imagem reutilizáveis por renderizadores Iced e PDF.
//! Nenhuma função altera os bytes originais nem posiciona o elemento na página.
//! Sombra e brilho são CAMADAS separadas; cabe ao consumidor compô-las.

use crate::{ErroImagem, ImagemRgba};
use image::imageops;

const MAX_BYTES: usize = 512 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErroVisual {
    ParametroInvalido,
    DimensoesInvalidas,
}

fn fator01(valor: f32) -> Result<f32, ErroVisual> {
    if valor.is_finite() && (0.0..=1.0).contains(&valor) {
        Ok(valor)
    } else {
        Err(ErroVisual::ParametroInvalido)
    }
}

fn buffer(largura: u32, altura: u32) -> Result<Vec<u8>, ErroVisual> {
    if largura == 0 || altura == 0 {
        return Err(ErroVisual::DimensoesInvalidas);
    }
    let bytes = (largura as usize)
        .checked_mul(altura as usize)
        .and_then(|n| n.checked_mul(4))
        .filter(|n| *n <= MAX_BYTES)
        .ok_or(ErroVisual::DimensoesInvalidas)?;
    let mut saida = Vec::new();
    saida.try_reserve_exact(bytes).map_err(|_| ErroVisual::DimensoesInvalidas)?;
    saida.resize(bytes, 0);
    Ok(saida)
}

fn imagem(largura: u32, altura: u32, dados: Vec<u8>) -> Result<ImagemRgba, ErroVisual> {
    ImagemRgba::nova(largura, altura, dados)
        .map_err(|_: ErroImagem| ErroVisual::DimensoesInvalidas)
}

impl ImagemRgba {
    /// Deforma o topo horizontalmente, mantendo o quadro e a altura.
    /// Mapeamento de vizinho mais próximo compatível com o FeatherSlide.
    /// A borda que sai da fotografia fica transparente.
    pub fn aplicar_perspectiva(&self, perspectiva: f32) -> Result<Self, ErroVisual> {
        if !perspectiva.is_finite() || !(-1.0..=1.0).contains(&perspectiva) {
            return Err(ErroVisual::ParametroInvalido);
        }
        if perspectiva.abs() <= 0.001 {
            return Ok(self.clone());
        }
        let (w, h) = self.dimensoes();
        let mut dados = buffer(w, h)?;
        let origem = self.pixels();
        for y in 0..h {
            let altura = y as f32 / h as f32;
            let proporcao = (1.0 - perspectiva * 0.65 * (1.0 - altura)).max(0.2);
            for x in 0..w {
                let u = ((x as f32 + 0.5) / w as f32 - 0.5) / proporcao + 0.5;
                if (0.0..1.0).contains(&u) {
                    let sx = (u * w as f32).floor().clamp(0.0, (w - 1) as f32) as usize;
                    let src = (y as usize * w as usize + sx) * 4;
                    let dst = (y as usize * w as usize + x as usize) * 4;
                    dados[dst..dst + 4].copy_from_slice(&origem[src..src + 4]);
                }
            }
        }
        imagem(w, h, dados)
    }

    /// Multiplica exclusivamente o alfa. Fator 1 preserva também RGB oculto.
    pub fn aplicar_opacidade(&self, opacidade: f32) -> Result<Self, ErroVisual> {
        let opacidade = fator01(opacidade)?;
        if opacidade == 1.0 {
            return Ok(self.clone());
        }
        let (w, h) = self.dimensoes();
        let mut dados = self.pixels().to_vec();
        for p in dados.chunks_exact_mut(4) {
            p[3] = (f32::from(p[3]) * opacidade).round() as u8;
        }
        imagem(w, h, dados)
    }

    /// Cria faixa espelhada de 1/3 da altura (mínimo de um pixel).
    /// A primeira linha vem da base da origem, a intensidade decai ao quadrado.
    /// A camada pertence ao espaço abaixo da imagem, sem sobrescrever o original.
    pub fn gerar_reflexo(&self, intensidade: f32) -> Result<Self, ErroVisual> {
        let intensidade = fator01(intensidade)?;
        let (w, h) = self.dimensoes();
        let altura = (h / 3).max(1);
        let mut dados = buffer(w, altura)?;
        let origem = self.pixels();
        for y in 0..altura {
            let fracao = y as f32 / altura as f32;
            let sy = h - 1 - y;
            let peso = intensidade * (1.0 - fracao).powi(2) * 0.7;
            for x in 0..w {
                let src = (sy as usize * w as usize + x as usize) * 4;
                let dst = (y as usize * w as usize + x as usize) * 4;
                dados[dst..dst + 4].copy_from_slice(&origem[src..src + 4]);
                dados[dst + 3] = (f32::from(origem[src + 3]) * peso).round() as u8;
            }
        }
        imagem(w, altura, dados)
    }

    /// Retorna camada de brilho externo dourado e margem em cada lado, em pixels.
    /// A GUI/PDF posiciona a camada em (x - margem, y - margem).
    /// Um gaussiano sobre o alfa evita importar o RGB original para o halo.
    pub fn gerar_brilho_externo(
        &self,
        intensidade: f32,
    ) -> Result<(Self, u32), ErroVisual> {
        let intensidade = fator01(intensidade)?;
        let (w, h) = self.dimensoes();
        let margem = ((w.min(h) as f32 * 0.08 * intensidade.max(0.2)).round() as u32)
            .clamp(4, 72);
        let largura = w.checked_add(margem.checked_mul(2).ok_or(ErroVisual::DimensoesInvalidas)?)
            .ok_or(ErroVisual::DimensoesInvalidas)?;
        let altura = h.checked_add(margem.checked_mul(2).ok_or(ErroVisual::DimensoesInvalidas)?)
            .ok_or(ErroVisual::DimensoesInvalidas)?;
        let mut dados = buffer(largura, altura)?;
        for y in 0..h {
            for x in 0..w {
                let src = (y as usize * w as usize + x as usize) * 4;
                let dst = ((y + margem) as usize * largura as usize + (x + margem) as usize) * 4;
                dados[dst..dst + 4].copy_from_slice(&[255, 232, 135, self.pixels()[src + 3]]);
            }
        }
        let base = imagem(largura, altura, dados)?.em_image();
        let mut camada = imageops::blur(&base, (margem as f32 * 0.65).max(1.0));
        for p in camada.pixels_mut() {
            // Desfoque em RGBA não pré-multiplicado escureceria o amarelo
            // nas bordas transparentes. O halo usa sempre a mesma cor.
            p.0[..3].copy_from_slice(&[255, 232, 135]);
            p.0[3] = (f32::from(p.0[3]) * intensidade).round() as u8;
        }
        Ok((Self::de_image(camada).map_err(|_| ErroVisual::DimensoesInvalidas)?, margem))
    }

    /// Sombra RGBA na mesma área da imagem, sem deslocamento: deslocar cabe à GUI.
    /// O raio (sigma gaussiano) segue o limite visual do Slide (0..28).
    /// O alfa da cor multiplica o alfa original, independentemente dos canais RGB.
    pub fn gerar_sombra(&self, cor: [u8; 4], raio: f32) -> Result<Self, ErroVisual> {
        if !raio.is_finite() || !(0.0..=28.0).contains(&raio) {
            return Err(ErroVisual::ParametroInvalido);
        }
        let (w, h) = self.dimensoes();
        let mut dados = buffer(w, h)?;
        for (dst, src) in dados.chunks_exact_mut(4).zip(self.pixels().chunks_exact(4)) {
            dst.copy_from_slice(&[cor[0], cor[1], cor[2], src[3]]);
        }
        let base = imagem(w, h, dados)?.em_image();
        let mut camada = if raio > 0.05 {
            imageops::blur(&base, raio.max(0.1))
        } else {
            base
        };
        for p in camada.pixels_mut() {
            p.0[..3].copy_from_slice(&cor[..3]);
            p.0[3] = ((u16::from(p.0[3]) * u16::from(cor[3]) + 127) / 255) as u8;
        }
        Self::de_image(camada).map_err(|_| ErroVisual::DimensoesInvalidas)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn exemplo() -> ImagemRgba {
        ImagemRgba::nova(2, 2, vec![
            255, 0, 0, 0, 0, 255, 0, 128,
            0, 0, 255, 255, 240, 230, 220, 255,
        ]).unwrap()
    }

    #[test]
    fn opacidade_preserva_rgb_oculto_e_nao_altera_fonte() {
        let img = exemplo();
        let original = img.clone();
        assert_eq!(img.aplicar_opacidade(1.0).unwrap(), img);
        let meio = img.aplicar_opacidade(0.5).unwrap();
        assert_eq!(&meio.pixels()[..4], &[255, 0, 0, 0]);
        assert_eq!(meio.pixels()[7], 64);
        assert_eq!(meio.pixels()[11], 128);
        assert_eq!(img.aplicar_opacidade(0.0).unwrap().pixels()[15], 0);
        assert_eq!(img, original);
    }

    #[test]
    fn reflexo_espelha_base_e_reduz_alpha() {
        let img = ImagemRgba::nova(1, 6, vec![
            1, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255,
            10, 11, 12, 255, 13, 14, 15, 255, 16, 17, 18, 128,
        ]).unwrap();
        let r = img.gerar_reflexo(1.0).unwrap();
        assert_eq!(r.dimensoes(), (1, 2));
        assert_eq!(&r.pixels()[..4], &[16, 17, 18, 90]);
        assert_eq!(&r.pixels()[4..8], &[13, 14, 15, 45]);
        assert_eq!(img.dimensoes(), (1, 6));
    }

    #[test]
    fn perspectiva_mantem_dimensoes_e_gerencia_bordas() {
        let img = ImagemRgba::nova(8, 2, [255, 80, 20, 255].repeat(16)).unwrap();
        assert_eq!(img.aplicar_perspectiva(0.0).unwrap(), img);
        let p = img.aplicar_perspectiva(1.0).unwrap();
        assert_eq!(p.dimensoes(), (8, 2));
        assert_eq!(p.pixels()[3], 0);
        assert_eq!(p.pixels()[7 * 4 + 3], 0);
        assert_eq!(p.pixels()[8 * 4 + 3], 0);
        assert_eq!(p.pixels()[9 * 4 + 3], 255);
        assert_eq!(img.pixels()[3], 255);
    }

    #[test]
    fn sombra_respeita_cor_alfa_e_blur_sem_sangramento_rgb() {
        let img = exemplo();
        let sombra = img.gerar_sombra([10, 20, 30, 128], 0.0).unwrap();
        assert_eq!(&sombra.pixels()[..4], &[10, 20, 30, 0]);
        assert_eq!(&sombra.pixels()[4..8], &[10, 20, 30, 64]);
        assert_eq!(sombra.pixels()[11], 128);
        let suave = img.gerar_sombra([1, 2, 3, 255], 2.0).unwrap();
        assert_eq!(suave.dimensoes(), img.dimensoes());
        assert!(suave.pixels().chunks_exact(4).all(|p| p[..3] == [1, 2, 3]));
        assert_eq!(img.pixels()[0], 255);
    }

    #[test]
    fn brilho_expande_quadro_e_mantem_alfa_independente() {
        let img = ImagemRgba::nova(4, 4, [20, 30, 40, 255].repeat(16)).unwrap();
        let (brilho, margem) = img.gerar_brilho_externo(1.0).unwrap();
        assert_eq!(margem, 4);
        assert_eq!(brilho.dimensoes(), (12, 12));
        assert!(brilho.pixels().chunks_exact(4).all(|p| p[..3] == [255, 232, 135]));
        assert!(brilho.pixels().chunks_exact(4).any(|p| p[3] != 0));
        assert_eq!(img.pixels()[0], 20);
        let (apagado, _) = img.gerar_brilho_externo(0.0).unwrap();
        assert!(apagado.pixels().chunks_exact(4).all(|p| p[3] == 0));
    }

    #[test]
    fn rejeita_parametros_ilegais_e_dimensoes_excessivas() {
        let img = exemplo();
        assert_eq!(img.aplicar_opacidade(f32::NAN), Err(ErroVisual::ParametroInvalido));
        assert_eq!(img.gerar_reflexo(-0.1), Err(ErroVisual::ParametroInvalido));
        assert_eq!(img.aplicar_perspectiva(3.0), Err(ErroVisual::ParametroInvalido));
        assert_eq!(img.gerar_sombra([0; 4], f32::INFINITY), Err(ErroVisual::ParametroInvalido));
        assert_eq!(img.gerar_brilho_externo(1.1), Err(ErroVisual::ParametroInvalido));
        assert_eq!(buffer(u32::MAX, u32::MAX), Err(ErroVisual::DimensoesInvalidas));
    }
}
