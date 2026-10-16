//! Exemplo IMG-008: envia um mosaico PPM P6 ao stdout, sem dependências de codec.
//! Uso: cargo run -p feather-image --example validacao_efeitos > /tmp/efeitos.ppm
use feather_image::ImagemRgba;
use std::io::{self, Write};

const PW: usize = 120;
const PH: usize = 112;
const W: usize = PW * 3;
const H: usize = PH * 2;

fn desenhar(destino: &mut [u8], imagem: &ImagemRgba, x0: usize, y0: usize) {
    let (w, h) = imagem.dimensoes();
    for y in 0..h as usize {
        for x in 0..w as usize {
            if x0 + x >= W || y0 + y >= H { continue; }
            let src = (y * w as usize + x) * 4;
            let dst = ((y0 + y) * W + x0 + x) * 3;
            let alfa = u16::from(imagem.pixels()[src + 3]);
            for c in 0..3 {
                let atual = u16::from(destino[dst + c]);
                let novo = u16::from(imagem.pixels()[src + c]);
                destino[dst + c] = ((novo * alfa + atual * (255 - alfa) + 127) / 255) as u8;
            }
        }
    }
}

fn main() -> io::Result<()> {
    let mut rgba = Vec::with_capacity(64 * 48 * 4);
    for y in 0..48i32 {
        for x in 0..64i32 {
            let dx = x - 31;
            let dy = y - 23;
            let alfa = if dx * dx + dy * dy < 18 * 18 { 255 } else { 0 };
            rgba.extend_from_slice(&[
                (30 + x * 2) as u8,
                (80 + y * 3) as u8,
                190,
                alfa,
            ]);
        }
    }
    let origem = ImagemRgba::nova(64, 48, rgba).expect("imagem demo RGBA");
    let perspectiva = origem.aplicar_perspectiva(0.85).expect("perspectiva");
    let opacidade = origem.aplicar_opacidade(0.40).expect("opacidade");
    let reflexo = origem.gerar_reflexo(0.9).expect("reflexo");
    let sombra = origem.gerar_sombra([20, 28, 44, 180], 4.0).expect("sombra");
    let (brilho, margem) = origem.gerar_brilho_externo(1.0).expect("glow");
    assert_eq!(origem.dimensoes(), (64, 48));
    assert_eq!(perspectiva.dimensoes(), origem.dimensoes());
    assert_eq!(reflexo.dimensoes(), (64, 16));
    assert!(margem >= 4);

    // Quadriculado mostra com clareza os três níveis de alfa.
    let mut ppm = vec![0u8; W * H * 3];
    for y in 0..H {
        for x in 0..W {
            let claro = (x / 10 + y / 10) % 2 == 0;
            let base = if claro { 245 } else { 210 };
            let i = (y * W + x) * 3;
            ppm[i..i + 3].copy_from_slice(&[base; 3]);
        }
    }
    // Linha 1: origem, perspectiva, opacidade.
    desenhar(&mut ppm, &origem, 28, 25);
    desenhar(&mut ppm, &perspectiva, PW + 28, 25);
    desenhar(&mut ppm, &opacidade, 2 * PW + 28, 25);
    // Linha 2: sombra, brilho, reflexo.
    desenhar(&mut ppm, &sombra, 28 + 6, PH + 25 + 6);
    desenhar(&mut ppm, &origem, 28, PH + 25);
    desenhar(&mut ppm, &brilho, PW + 28 - margem as usize, PH + 25 - margem as usize);
    desenhar(&mut ppm, &origem, PW + 28, PH + 25);
    desenhar(&mut ppm, &origem, 2 * PW + 28, PH + 15);
    desenhar(&mut ppm, &reflexo, 2 * PW + 28, PH + 15 + 48);

    let mut out = io::stdout().lock();
    out.write_all(format!("P6\n{W} {H}\n255\n").as_bytes())?;
    out.write_all(&ppm)?;
    eprintln!(
        "IMG-008: mosaico PPM RGB {}x{} — origem, perspectiva, \
         opacidade, sombra, brilho externo e reflexo.",
        W, H
    );
    Ok(())
}
