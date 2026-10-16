//! Exporta RGBA da fixture de blur/mediana para inspeção visual reproduzível.
use feather_image::ImagemRgba;
fn main() {
    let pasta = std::env::args()
        .nth(1)
        .expect("informe diretório para os RGBA");
    std::fs::create_dir_all(&pasta).unwrap();
    let mut px = Vec::new();
    for y in 0..48 {
        for x in 0..64 {
            let v = if x < 32 { 32 } else { 220 };
            let ruido = if (x * 13 + y * 7) % 29 == 0 {
                255 - v
            } else {
                v
            };
            px.extend_from_slice(&[ruido, ruido, ruido, 255]);
        }
    }
    let origem = ImagemRgba::nova(64, 48, px).unwrap();
    for (nome, im) in [
        ("ruido-original", origem.clone()),
        ("blur", origem.desfocar(1)),
        ("mediana", origem.reduzir_ruido(1)),
    ] {
        assert_eq!(im.dimensoes(), origem.dimensoes());
        assert!(im.pixels().chunks_exact(4).all(|p| p[3] == 255));
        std::fs::write(
            std::path::Path::new(&pasta).join(format!("{nome}.rgba")),
            im.pixels(),
        )
        .unwrap();
    }
}
