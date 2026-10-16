//! Exemplo observável IMG-014: motor comum de efeitos Docs/Sheet -> PPM P6.
//! cargo run -p feather-image --example validacao_img014 -j 2 > /tmp/img014-efeitos.ppm
//! PPM é gerado sem codec extra e pode ser aberto por visualizadores Linux.
use feather_image::{EfeitoRapido, ImagemRgba};
use std::io::{self, Write};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    const W: u32 = 180;
    const H: u32 = 90;
    let mut pixels = Vec::with_capacity((W * H * 4) as usize);
    for y in 0..H {
        for x in 0..W {
            pixels.extend_from_slice(&[
                (x * 255 / W) as u8,
                (y * 255 / H) as u8,
                ((x + y) * 255 / (W + H)) as u8,
                if x < W / 2 { 100 } else { 255 },
            ]);
        }
    }
    let original = ImagemRgba::nova(W, H, pixels).unwrap();
    let saida = EfeitoRapido::Cinza.executar(&original).unwrap();
    assert_eq!(original.dimensoes(), saida.dimensoes());
    assert_eq!(saida.pixels()[3], 100);
    assert_eq!(saida.pixels()[(W * 4 - 1) as usize], 255);
    for (antes, depois) in original.pixels().chunks_exact(4)
        .zip(saida.pixels().chunks_exact(4)) {
        assert_eq!(antes[3], depois[3]);
    }
    let mut output = io::BufWriter::new(io::stdout().lock());
    write!(output, "P6\n{} {}\n255\n", W, H)?;
    for pixel in saida.pixels().chunks_exact(4) {
        output.write_all(&pixel[..3])?;
    }
    output.flush()?;
    eprintln!("IMG-014: efeito cinza do motor comum em {W}x{H}; alfa preservado. PPM no stdout.");
    Ok(())
}
