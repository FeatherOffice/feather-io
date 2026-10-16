//! Correções cromáticas neutras sobre RGBA8 intercalado.
//! Equações YIQ e filtros extraídos do FeatherSlide, preservando o canal alfa.
//! O filtro 0 é neutro; 1 cinza, 2 sépia, 3 negativo, 4 posterização,
//! 5 solarização (mesmos identificadores serializados em `.fsdz`).

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AjustesCor {
    pub brilho: f32,
    pub contraste: f32,
    pub saturacao: f32,
    pub temperatura: f32,
    pub matiz: f32,
    pub filtro: u8,
}

impl Default for AjustesCor {
    fn default() -> Self {
        Self {
            brilho: 0.0,
            contraste: 0.0,
            saturacao: 1.0,
            temperatura: 0.0,
            matiz: 0.0,
            filtro: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErroCor {
    PixelsInvalidos,
    ParametroInvalido,
}

impl AjustesCor {
    pub fn ativo(self) -> bool {
        self.brilho.abs() > 0.0001
            || self.contraste.abs() > 0.0001
            || (self.saturacao - 1.0).abs() > 0.0001
            || self.temperatura.abs() > 0.0001
            || self.matiz.abs() > 0.0001
            || self.filtro != 0
    }

    fn valido(self) -> bool {
        self.brilho.is_finite()
            && self.contraste.is_finite()
            && self.saturacao.is_finite()
            && self.temperatura.is_finite()
            && self.matiz.is_finite()
    }
}

/// Aplica filtros in-place a bytes RGBA, sem alocação ou conversão de formato.
/// Compatível com `image::RgbaImage::as_mut()` e `ImagemRgba::pixels_mut()`.
/// Pixels totalmente transparentes não mudam RGB, para preservar conteúdo oculto.
pub fn aplicar_cores_rgba(pixels: &mut [u8], cfg: AjustesCor) -> Result<(), ErroCor> {
    if pixels.len() % 4 != 0 {
        return Err(ErroCor::PixelsInvalidos);
    }
    if !cfg.valido() {
        return Err(ErroCor::ParametroInvalido);
    }
    if !cfg.ativo() {
        return Ok(());
    }

    let (sin, cos) = cfg.matiz.to_radians().sin_cos();
    for p in pixels.chunks_exact_mut(4) {
        let [r, g, b, a] = [p[0], p[1], p[2], p[3]];
        if a == 0 {
            continue;
        }
        let (mut r, mut g, mut b) = (
            f32::from(r) / 255.0,
            f32::from(g) / 255.0,
            f32::from(b) / 255.0,
        );
        // Rotação YIQ, mantendo exatamente a fórmula da GUI anterior.
        let luma = r * 0.299 + g * 0.587 + b * 0.114;
        let i = r * 0.596 - g * 0.275 - b * 0.321;
        let q = r * 0.212 - g * 0.523 + b * 0.311;
        let ii = i * cos - q * sin;
        let qq = i * sin + q * cos;
        r = luma + 0.956 * ii + 0.621 * qq;
        g = luma - 0.272 * ii - 0.647 * qq;
        b = luma - 1.106 * ii + 1.703 * qq;
        let lum = r * 0.299 + g * 0.587 + b * 0.114;
        r = lum + (r - lum) * cfg.saturacao;
        g = lum + (g - lum) * cfg.saturacao;
        b = lum + (b - lum) * cfg.saturacao;
        let contraste = (1.0 + cfg.contraste).max(0.0);
        let aplicar = |canal: f32, temperatura: f32| {
            ((canal - 0.5) * contraste + 0.5 + cfg.brilho * 0.45 + temperatura).clamp(0.0, 1.0)
        };
        r = aplicar(r, cfg.temperatura * 0.17);
        g = aplicar(g, cfg.temperatura * 0.025);
        b = aplicar(b, -cfg.temperatura * 0.17);
        match cfg.filtro {
            1 => {
                let y = r * 0.299 + g * 0.587 + b * 0.114;
                r = y;
                g = y;
                b = y;
            }
            2 => {
                let (or, og, ob) = (r, g, b);
                r = (or * 0.393 + og * 0.769 + ob * 0.189).min(1.0);
                g = (or * 0.349 + og * 0.686 + ob * 0.168).min(1.0);
                b = (or * 0.272 + og * 0.534 + ob * 0.131).min(1.0);
            }
            3 => {
                r = 1.0 - r;
                g = 1.0 - g;
                b = 1.0 - b;
            }
            4 => {
                r = (r * 4.0).round() / 4.0;
                g = (g * 4.0).round() / 4.0;
                b = (b * 4.0).round() / 4.0;
            }
            5 => {
                if r > 0.5 {
                    r = 1.0 - r;
                }
                if g > 0.5 {
                    g = 1.0 - g;
                }
                if b > 0.5 {
                    b = 1.0 - b;
                }
            }
            _ => {}
        }
        p[0] = (r.clamp(0.0, 1.0) * 255.0).round() as u8;
        p[1] = (g.clamp(0.0, 1.0) * 255.0).round() as u8;
        p[2] = (b.clamp(0.0, 1.0) * 255.0).round() as u8;
    }
    Ok(())
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn filtros_artistico_e_alpha_0_128_255() {
        for filtro in 0..=5 {
            let mut px = [19, 31, 47, 0, 120, 34, 200, 128, 70, 90, 220, 255];
            let original = px;
            aplicar_cores_rgba(
                &mut px,
                AjustesCor {
                    filtro,
                    brilho: 0.2,
                    ..AjustesCor::default()
                },
            )
            .unwrap();
            assert_eq!(&px[..4], &original[..4]);
            assert_eq!(px[7], 128);
            assert_eq!(px[11], 255);
            if filtro == 1 {
                assert_eq!(px[4], px[5]);
                assert_eq!(px[5], px[6]);
            }
        }
    }

    #[test]
    fn parametros_neutros_nao_modificam_original() {
        let mut px = [1, 2, 3, 128, 254, 253, 252, 255];
        let original = px;
        aplicar_cores_rgba(&mut px, AjustesCor::default()).unwrap();
        assert_eq!(px, original);
    }

    #[test]
    fn rejeita_nan_e_buffer_incompleto_sem_quebrar_filtros_desconhecidos() {
        let mut px = [1, 2, 3, 255];
        assert_eq!(
            aplicar_cores_rgba(
                &mut px,
                AjustesCor {
                    matiz: f32::NAN,
                    ..AjustesCor::default()
                }
            ),
            Err(ErroCor::ParametroInvalido)
        );
        assert_eq!(
            aplicar_cores_rgba(
                &mut px,
                AjustesCor {
                    filtro: 9,
                    ..AjustesCor::default()
                }
            ),
            Ok(())
        );
        assert_eq!(
            aplicar_cores_rgba(&mut px[..3], AjustesCor::default()),
            Err(ErroCor::PixelsInvalidos)
        );
    }
}
