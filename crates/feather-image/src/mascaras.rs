//! Máscaras alfa não destrutivas para recorte suave e remoção de fundo.
//! A GUI escolhe quando compor o resultado, preservando os bytes originais.

use crate::ImagemRgba;
use std::collections::VecDeque;

/// Alinhado ao teto de 512 MiB para o buffer RGBA8 de origem.
const LIMITE_PIXELS: usize = 128 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErroMascara {
    DimensoesInvalidas,
    BufferInvalido,
    DimensoesDiferentes,
    AreaInvalida,
    SuavizacaoInvalida,
}

/// Um byte de cobertura por pixel, independente dos pixels RGBA de origem.
/// 0 = invisível, 255 = cobertura total. Não altera o arquivo original.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MascaraAlfa {
    largura: u32,
    altura: u32,
    alfa: Vec<u8>,
}

impl MascaraAlfa {
    pub fn nova(largura: u32, altura: u32, alfa: Vec<u8>) -> Result<Self, ErroMascara> {
        let esperado = (largura as usize)
            .checked_mul(altura as usize)
            .ok_or(ErroMascara::DimensoesInvalidas)?;
        if largura == 0 || altura == 0 {
            return Err(ErroMascara::DimensoesInvalidas);
        }
        if esperado > LIMITE_PIXELS {
            return Err(ErroMascara::DimensoesInvalidas);
        }
        if alfa.len() != esperado {
            return Err(ErroMascara::BufferInvalido);
        }
        Ok(Self { largura, altura, alfa })
    }

    pub fn dimensoes(&self) -> (u32, u32) {
        (self.largura, self.altura)
    }

    pub fn alfa(&self) -> &[u8] {
        &self.alfa
    }

    /// Recorte retangular em coordenadas de pixel da imagem ORIGINAL.
    /// Não descarta bytes: a máscara cobre o quadro original inteiro.
    /// `suavizacao` é a meia-largura (px) da transição junto à borda;
    /// zero corresponde a corte duro. A área pode ultrapassar o quadro.
    pub fn retangular(
        largura: u32,
        altura: u32,
        x: f32,
        y: f32,
        area_largura: f32,
        area_altura: f32,
        suavizacao: f32,
    ) -> Result<Self, ErroMascara> {
        if largura == 0 || altura == 0 {
            return Err(ErroMascara::DimensoesInvalidas);
        }
        if ![x, y, area_largura, area_altura]
            .iter()
            .all(|n| n.is_finite())
            || area_largura <= 0.0
            || area_altura <= 0.0
            || !((x as f64 + area_largura as f64).is_finite()
                && (y as f64 + area_altura as f64).is_finite())
        {
            return Err(ErroMascara::AreaInvalida);
        }
        if !suavizacao.is_finite() || !(0.0..=256.0).contains(&suavizacao) {
            return Err(ErroMascara::SuavizacaoInvalida);
        }
        let total = (largura as usize)
            .checked_mul(altura as usize)
            .ok_or(ErroMascara::DimensoesInvalidas)?;
        let direita = x as f64 + area_largura as f64;
        let baixo = y as f64 + area_altura as f64;
        if total > LIMITE_PIXELS {
            return Err(ErroMascara::DimensoesInvalidas);
        }
        let mut alfa = Vec::with_capacity(total);
        for py in 0..altura {
            let cy = f64::from(py) + 0.5;
            for px in 0..largura {
                let cx = f64::from(px) + 0.5;
                let dentro = cx >= f64::from(x)
                    && cx < direita
                    && cy >= f64::from(y)
                    && cy < baixo;
                if suavizacao == 0.0 {
                    alfa.push(if dentro { 255 } else { 0 });
                    continue;
                }
                // Distância assinada: negativa dentro e positiva fora.
                let dx = (f64::from(x) - cx).max(cx - direita);
                let dy = (f64::from(y) - cy).max(cy - baixo);
                let dist = if dentro {
                    -((cx - f64::from(x))
                        .min(direita - cx)
                        .min(cy - f64::from(y))
                        .min(baixo - cy))
                } else {
                    dx.max(0.0).hypot(dy.max(0.0))
                };
                let cobertura = (0.5 - dist / (2.0 * f64::from(suavizacao)))
                    .clamp(0.0, 1.0);
                alfa.push((cobertura * 255.0).round() as u8);
            }
        }
        Ok(Self { largura, altura, alfa })
    }

    /// Remove apenas pixels próximos da cor que são alcançáveis pelas bordas.
    /// Um objeto fechado pode conter a mesma cor sem que seu interior suma.
    /// A distância é o maior desvio RGB; `tolerancia` é alfa=0 e
    /// `transicao` controla a faixa de alfa crescente (antialias).
    /// Não é segmentação semântica por IA e não altera os bytes originais.
    pub fn fundo_por_cor(
        imagem: &ImagemRgba,
        cor: [u8; 3],
        tolerancia: u8,
        transicao: u8,
    ) -> Self {
        let (largura, altura) = imagem.dimensoes();
        let (w, h) = (largura as usize, altura as usize);
        let pixels = imagem.pixels();
        let total = w * h;
        let mut alfa = vec![255u8; total];
        let mut visitado = vec![false; total];
        let mut fila = VecDeque::new();
        let maximo = tolerancia.saturating_add(transicao);

        fn desvio(pixels: &[u8], indice: usize, cor: [u8; 3]) -> u8 {
            let offset = indice * 4;
            (0..3)
                .map(|canal| pixels[offset + canal].abs_diff(cor[canal]))
                .max()
                .unwrap_or(0)
        }
        fn tentar(
            indice: usize,
            pixels: &[u8],
            cor: [u8; 3],
            maximo: u8,
            visitado: &mut [bool],
            fila: &mut VecDeque<usize>,
        ) {
            if visitado[indice] {
                return;
            }
            visitado[indice] = true;
            if desvio(pixels, indice, cor) <= maximo {
                fila.push_back(indice);
            }
        }

        for x in 0..w {
            tentar(x, pixels, cor, maximo, &mut visitado, &mut fila);
            tentar((h - 1) * w + x, pixels, cor, maximo, &mut visitado, &mut fila);
        }
        for y in 0..h {
            tentar(y * w, pixels, cor, maximo, &mut visitado, &mut fila);
            tentar(y * w + w - 1, pixels, cor, maximo, &mut visitado, &mut fila);
        }
        while let Some(i) = fila.pop_front() {
            let distancia = desvio(pixels, i, cor);
            alfa[i] = if distancia <= tolerancia {
                0
            } else if transicao == 0 {
                255
            } else {
                let fracao = u16::from(distancia - tolerancia) * 255;
                ((fracao + u16::from(transicao) / 2) / u16::from(transicao)) as u8
            };
            let x = i % w;
            let y = i / w;
            if x > 0 {
                tentar(i - 1, pixels, cor, maximo, &mut visitado, &mut fila);
            }
            if x + 1 < w {
                tentar(i + 1, pixels, cor, maximo, &mut visitado, &mut fila);
            }
            if y > 0 {
                tentar(i - w, pixels, cor, maximo, &mut visitado, &mut fila);
            }
            if y + 1 < h {
                tentar(i + w, pixels, cor, maximo, &mut visitado, &mut fila);
            }
        }
        Self { largura, altura, alfa }
    }

    /// Interseção de máscaras com multiplicação de alfa, sem alterar entradas.
    pub fn combinar(&self, outra: &Self) -> Result<Self, ErroMascara> {
        if self.dimensoes() != outra.dimensoes() {
            return Err(ErroMascara::DimensoesDiferentes);
        }
        let alfa = self.alfa.iter().zip(&outra.alfa)
            .map(|(a, b)| ((u16::from(*a) * u16::from(*b) + 127) / 255) as u8)
            .collect();
        Ok(Self { largura: self.largura, altura: self.altura, alfa })
    }

    /// Gera uma visualização RGBA independente. Multiplica, não substitui,
    /// o alfa original e mantém os canais RGB (inclusive pixels transparentes).
    pub fn aplicar(&self, imagem: &ImagemRgba) -> Result<ImagemRgba, ErroMascara> {
        if self.dimensoes() != imagem.dimensoes() {
            return Err(ErroMascara::DimensoesDiferentes);
        }
        let mut pixels = imagem.pixels().to_vec();
        for (pixel, alfa) in pixels.chunks_exact_mut(4).zip(&self.alfa) {
            pixel[3] = ((u16::from(pixel[3]) * u16::from(*alfa) + 127) / 255) as u8;
        }
        ImagemRgba::nova(self.largura, self.altura, pixels)
            .map_err(|_| ErroMascara::BufferInvalido)
    }
}
