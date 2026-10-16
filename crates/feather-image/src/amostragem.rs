//! Redimensionamento de RGBA em espaço de pixels, sem GUI e sem codecs.
//! Filtros separáveis calculados diretamente no destino, sem cópias da origem.
//! A interpolação utiliza alfa pré-multiplicado para evitar halos nas bordas.

use crate::{ErroImagem, ImagemRgba};

/// Bilinear (triângulo) ou bicúbico Catmull–Rom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interpolacao {
    Bilinear,
    Bicubica,
}

const LIMITE_SAIDA_BYTES: usize = 512 * 1024 * 1024;

fn kernel(distancia: f64, filtro: Interpolacao) -> f64 {
    let x = distancia.abs();
    match filtro {
        Interpolacao::Bilinear => (1.0 - x).max(0.0),
        Interpolacao::Bicubica if x < 1.0 => 1.5 * x * x * x - 2.5 * x * x + 1.0,
        Interpolacao::Bicubica if x < 2.0 => -0.5 * x * x * x + 2.5 * x * x - 4.0 * x + 2.0,
        Interpolacao::Bicubica => 0.0,
    }
}

fn eixo(destino: u32, origem: u32, novo: u32, filtro: Interpolacao) -> Vec<(usize, f64)> {
    let escala = f64::from(origem) / f64::from(novo);
    // Quando reduzimos, ampliamos o suporte do filtro para antialiasing.
    let alcance = escala.max(1.0);
    let raio = match filtro {
        Interpolacao::Bilinear => 1.0,
        Interpolacao::Bicubica => 2.0,
    } * alcance;
    let centro = (f64::from(destino) + 0.5) * escala - 0.5;
    let inicio = ((centro - raio).floor() as i64).max(0);
    let fim = ((centro + raio).ceil() as i64).min(i64::from(origem) - 1);
    let mut pesos = Vec::with_capacity((fim - inicio + 1).max(0) as usize);
    for indice in inicio..=fim {
        let peso = kernel((indice as f64 - centro) / alcance, filtro);
        if peso != 0.0 {
            pesos.push((indice as usize, peso));
        }
    }
    // Evita escurecer bordas, onde o kernel extrapolaria a imagem.
    let normalizador: f64 = pesos.iter().map(|(_, p)| *p).sum();
    if normalizador.abs() > 1e-12 {
        for (_, peso) in &mut pesos {
            *peso /= normalizador;
        }
    } else {
        pesos.clear();
        pesos.push((
            (centro.round() as i64).clamp(0, i64::from(origem) - 1) as usize,
            1.0,
        ));
    }
    pesos
}

impl ImagemRgba {
    /// Redimensiona com antialiasing na redução e alfa pré-multiplicado.
    ///
    /// - Não altera `self`, nem os bytes originais usados para decodificação.
    /// - Centro de pixel alinhado: `(dest + 0.5) * src / dst - 0.5`.
    /// - Pixels completamente transparentes na saída recebem RGB zero.
    /// - Uma saída maior que 512 MiB é recusada antes de alocar memória.
    pub fn redimensionar_interpolado(
        &self,
        largura: u32,
        altura: u32,
        interpolacao: Interpolacao,
    ) -> Result<Self, ErroImagem> {
        if largura == 0 || altura == 0 {
            return Err(ErroImagem::DimensoesInvalidas);
        }
        if (largura, altura) == self.dimensoes() {
            return Ok(self.clone()); // Preserva até o RGB oculto, sem arredondamento.
        }
        let len = (largura as usize)
            .checked_mul(altura as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .filter(|bytes| *bytes <= LIMITE_SAIDA_BYTES)
            .ok_or(ErroImagem::DimensoesInvalidas)?;
        let mut dados = Vec::new();
        dados
            .try_reserve_exact(len)
            .map_err(|_| ErroImagem::DimensoesInvalidas)?;
        dados.resize(len, 0);
        let w = self.largura as usize;
        let origem = self.pixels();
        // Cache de pesos horizontais: eles são iguais para cada linha.
        let pesos_x: Vec<_> = (0..largura)
            .map(|x| eixo(x, self.largura, largura, interpolacao))
            .collect();
        for y in 0..altura {
            let pesos_y = eixo(y, self.altura, altura, interpolacao);
            for x in 0..largura {
                let mut premult = [0.0_f64; 3];
                let mut alpha = 0.0_f64;
                for &(sy, wy) in &pesos_y {
                    for &(sx, wx) in &pesos_x[x as usize] {
                        let peso = wx * wy;
                        let src = (sy * w + sx) * 4;
                        let a = f64::from(origem[src + 3]) / 255.0;
                        alpha += a * peso;
                        for c in 0..3 {
                            premult[c] += f64::from(origem[src + c]) * a * peso;
                        }
                    }
                }
                let dst = (y as usize * largura as usize + x as usize) * 4;
                if alpha > 1e-12 {
                    for c in 0..3 {
                        dados[dst + c] = (premult[c] / alpha).clamp(0.0, 255.0).round() as u8;
                    }
                }
                dados[dst + 3] = (alpha * 255.0).clamp(0.0, 255.0).round() as u8;
            }
        }
        Self::nova(largura, altura, dados)
    }

    /// Bilinear com antialiasing e preservação de alfa.
    pub fn redimensionar_bilinear(&self, largura: u32, altura: u32) -> Result<Self, ErroImagem> {
        self.redimensionar_interpolado(largura, altura, Interpolacao::Bilinear)
    }

    /// Bicúbico Catmull–Rom com antialiasing e preservação de alfa.
    pub fn redimensionar_bicubico(&self, largura: u32, altura: u32) -> Result<Self, ErroImagem> {
        self.redimensionar_interpolado(largura, altura, Interpolacao::Bicubica)
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    fn imagem() -> ImagemRgba {
        ImagemRgba::nova(
            2,
            2,
            vec![
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
            ],
        )
        .unwrap()
    }

    #[test]
    fn bilinear_interpolacao_centro_quatro_cantos() {
        let entrada = imagem();
        let original = entrada.pixels().to_vec();
        let saida = entrada.redimensionar_bilinear(3, 3).unwrap();
        assert_eq!(saida.dimensoes(), (3, 3));
        assert_eq!(&saida.pixels()[0..4], &[255, 0, 0, 255]);
        assert_eq!(&saida.pixels()[(3 * 3 - 1) * 4..], &[255, 255, 255, 255]);
        assert_eq!(&saida.pixels()[16..20], &[128, 128, 128, 255]);
        assert_eq!(entrada.pixels(), original);
    }

    #[test]
    fn interpolacao_premultiplicada_evita_cor_fantasma() {
        let entrada = ImagemRgba::nova(2, 1, vec![255, 0, 0, 255, 0, 255, 0, 0]).unwrap();
        for saida in [
            entrada.redimensionar_bilinear(3, 1).unwrap(),
            entrada.redimensionar_bicubico(3, 1).unwrap(),
        ] {
            let meio = &saida.pixels()[4..8];
            assert_eq!(meio[0], 255);
            assert_eq!(meio[1], 0);
            assert_eq!(meio[2], 0);
            assert!(meio[3] >= 120 && meio[3] <= 140);
            assert_eq!(saida.pixels()[11], 0);
        }
        assert_eq!(&entrada.pixels()[4..8], &[0, 255, 0, 0]);
    }

    #[test]
    fn identidade_e_dimensoes_invalidas() {
        let entrada = imagem();
        for filtro in [Interpolacao::Bilinear, Interpolacao::Bicubica] {
            assert_eq!(
                entrada.redimensionar_interpolado(2, 2, filtro).unwrap(),
                entrada
            );
            assert_eq!(
                entrada.redimensionar_interpolado(0, 2, filtro),
                Err(ErroImagem::DimensoesInvalidas)
            );
            assert_eq!(
                entrada.redimensionar_interpolado(u32::MAX, u32::MAX, filtro),
                Err(ErroImagem::DimensoesInvalidas)
            );
        }
    }

    #[test]
    fn reducao_antialiasing_e_constante_estavel() {
        let constante = ImagemRgba::nova(17, 13, [90, 120, 200, 128].repeat(17 * 13)).unwrap();
        for filtro in [Interpolacao::Bilinear, Interpolacao::Bicubica] {
            let menor = constante.redimensionar_interpolado(2, 1, filtro).unwrap();
            assert_eq!(menor.pixels(), &[90, 120, 200, 128, 90, 120, 200, 128]);
        }
    }
}
