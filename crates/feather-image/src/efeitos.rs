//! Operações de nitidez, desfocagem e redução de ruído, independentes de GUI.
//! Todos os algoritmos operam exclusivamente nos canais RGB e preservam alfa.

use crate::ImagemRgba;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErroEfeito {
    BufferInvalido,
    IntensidadeInvalida,
}

/// Máscara de nitidez com imagem previamente desfocada (mesmo layout RGBA8).
/// Receber o blur pronto permite manter a implementação gaussiana da GUI Slide
/// durante a migração, sem cópia extra de pixels.
pub fn realcar_detalhes_rgba(
    pixels: &mut [u8],
    desfocado: &[u8],
    intensidade: f32,
) -> Result<(), ErroEfeito> {
    if pixels.len() != desfocado.len() || pixels.len() % 4 != 0 {
        return Err(ErroEfeito::BufferInvalido);
    }
    if !intensidade.is_finite() || !(0.0..=1.0).contains(&intensidade) {
        return Err(ErroEfeito::IntensidadeInvalida);
    }
    if intensidade <= 0.001 {
        return Ok(());
    }
    for (atual, suave) in pixels.chunks_exact_mut(4).zip(desfocado.chunks_exact(4)) {
        for c in 0..3 {
            let valor = f32::from(atual[c])
                + (f32::from(atual[c]) - f32::from(suave[c])) * intensidade * 2.2;
            atual[c] = valor.clamp(0.0, 255.0).round() as u8;
        }
    }
    Ok(())
}

impl ImagemRgba {
    /// Box blur separável, com raios em pixels limitados a 64.
    /// Complexidade O(largura * altura), duas alocações RGBA e prefixos reutilizados.
    /// Bordas usam apenas os vizinhos válidos; canal alfa fica intocado.
    pub fn desfocar(&self, raio: u32) -> Self {
        let raio = raio.min(64) as usize;
        if raio == 0 {
            return self.clone();
        }
        let (w, h) = (self.dimensoes().0 as usize, self.dimensoes().1 as usize);
        let entrada = self.pixels();
        let mut horizontal = entrada.to_vec();
        // Prefixos por linha reutilizados; somas u64 evitam overflow.
        let mut soma = vec![[0u64; 3]; w + 1];
        for y in 0..h {
            soma[0] = [0; 3];
            for x in 0..w {
                let i = (y * w + x) * 4;
                for c in 0..3 {
                    soma[x + 1][c] = soma[x][c] + u64::from(entrada[i + c]);
                }
            }
            for x in 0..w {
                let primeiro = x.saturating_sub(raio);
                let ultimo = x.saturating_add(raio).min(w - 1);
                let n = (ultimo - primeiro + 1) as u64;
                let i = (y * w + x) * 4;
                for c in 0..3 {
                    horizontal[i + c] =
                        ((soma[ultimo + 1][c] - soma[primeiro][c] + n / 2) / n) as u8;
                }
            }
        }
        let mut saida = entrada.to_vec();
        let mut soma = vec![[0u64; 3]; h + 1];
        for x in 0..w {
            soma[0] = [0; 3];
            for y in 0..h {
                let i = (y * w + x) * 4;
                for c in 0..3 {
                    soma[y + 1][c] = soma[y][c] + u64::from(horizontal[i + c]);
                }
            }
            for y in 0..h {
                let primeiro = y.saturating_sub(raio);
                let ultimo = y.saturating_add(raio).min(h - 1);
                let n = (ultimo - primeiro + 1) as u64;
                let i = (y * w + x) * 4;
                for c in 0..3 {
                    saida[i + c] = ((soma[ultimo + 1][c] - soma[primeiro][c] + n / 2) / n) as u8;
                }
            }
        }
        // Construção segura: mesmas dimensões e mesmo comprimento validado da origem.
        Self::nova(self.dimensoes().0, self.dimensoes().1, saida)
            .expect("desfocagem preserva dimensoes e comprimento RGBA")
    }

    /// Filtro de mediana local contra pixels isolados, raio 0 a 2 px.
    /// Não altera a transparência e não mistura bytes de pixels vizinhos.
    pub fn reduzir_ruido(&self, raio: u8) -> Self {
        let raio = usize::from(raio.min(2));
        if raio == 0 {
            return self.clone();
        }
        let (w, h) = (self.dimensoes().0 as usize, self.dimensoes().1 as usize);
        let original = self.pixels();
        let mut saida = original.to_vec();
        let mut vizinhos = [0u8; 25];
        for y in 0..h {
            for x in 0..w {
                let x0 = x.saturating_sub(raio);
                let x1 = x.saturating_add(raio).min(w - 1);
                let y0 = y.saturating_sub(raio);
                let y1 = y.saturating_add(raio).min(h - 1);
                let i = (y * w + x) * 4;
                for c in 0..3 {
                    let mut n = 0;
                    for yy in y0..=y1 {
                        for xx in x0..=x1 {
                            vizinhos[n] = original[(yy * w + xx) * 4 + c];
                            n += 1;
                        }
                    }
                    vizinhos[..n].sort_unstable();
                    saida[i + c] = vizinhos[n / 2];
                }
            }
        }
        Self::nova(self.dimensoes().0, self.dimensoes().1, saida)
            .expect("filtro de mediana preserva dimensoes e comprimento RGBA")
    }

    /// Nitidez independente de GUI, usando blur de 1 px como máscara.
    /// O Slide pode passar seu blur gaussiano ao `realcar_detalhes_rgba`
    /// para conservar a aparência atual durante a transição.
    pub fn realcar_detalhes(&mut self, intensidade: f32) -> Result<(), ErroEfeito> {
        if !intensidade.is_finite() || !(0.0..=1.0).contains(&intensidade) {
            return Err(ErroEfeito::IntensidadeInvalida);
        }
        if intensidade <= 0.001 {
            return Ok(());
        }
        let suave = self.desfocar(1);
        realcar_detalhes_rgba(&mut self.pixels, suave.pixels(), intensidade)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn blur_preserva_alfa_em_zero_meio_e_opaco() {
        let im =
            ImagemRgba::nova(3, 1, vec![0, 0, 0, 0, 255, 255, 255, 128, 0, 0, 0, 255]).unwrap();
        let bl = im.desfocar(1);
        assert_eq!(
            bl.pixels(),
            &[128, 128, 128, 0, 85, 85, 85, 128, 128, 128, 128, 255,]
        );
        assert_eq!(im.desfocar(0), im);
    }

    #[test]
    fn mediana_suprime_ponto_ruidoso_sem_mexer_no_alfa() {
        let mut pixels = vec![0u8; 3 * 3 * 4];
        for p in pixels.chunks_exact_mut(4) {
            p[3] = 128;
        }
        pixels[4 * 4] = 255;
        pixels[4 * 4 + 3] = 255;
        let im = ImagemRgba::nova(3, 3, pixels).unwrap();
        let result = im.reduzir_ruido(1);
        assert_eq!(result.pixels()[4 * 4], 0);
        assert_eq!(result.pixels()[4 * 4 + 3], 255);
        assert_eq!(result.pixels()[3], 128);
    }

    #[test]
    fn mascara_de_nitidez_preserva_alfa_e_valida_parametro() {
        let mut px = [100, 80, 60, 128];
        assert_eq!(
            realcar_detalhes_rgba(&mut px, &[90, 70, 50, 2], 0.5),
            Ok(())
        );
        assert_eq!(px, [111, 91, 71, 128]);
        assert_eq!(
            realcar_detalhes_rgba(&mut px, &[0; 4], f32::NAN),
            Err(ErroEfeito::IntensidadeInvalida)
        );
        assert_eq!(
            realcar_detalhes_rgba(&mut px, &[0; 3], 0.5),
            Err(ErroEfeito::BufferInvalido)
        );
    }
}
