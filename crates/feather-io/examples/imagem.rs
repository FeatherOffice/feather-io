//! Fixture RGBA: recorte, espelhamento, filtros e escala com alfa preservado.
use feather_image::ImagemRgba;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let texto = include_str!("../../../fixtures/imagem.rgba.txt");
    let numeros: Vec<u32> = texto
        .lines()
        .filter(|linha| !linha.trim_start().starts_with('#'))
        .flat_map(str::split_whitespace)
        .map(str::parse)
        .collect::<Result<_, _>>()?;
    let (largura, altura) = (numeros[0], numeros[1]);
    let pixels: Vec<u8> = numeros[2..]
        .iter()
        .map(|&n| u8::try_from(n))
        .collect::<Result<_, _>>()?;
    let mut imagem = ImagemRgba::nova(largura, altura, pixels).map_err(|e| format!("{e:?}"))?;
    imagem.espelhar_horizontal();
    assert_eq!(&imagem.pixels()[..4], &[0, 255, 0, 128]);
    let recorte = imagem.cortar(0, 0, 1, 2).map_err(|e| format!("{e:?}"))?;
    assert_eq!(recorte.dimensoes(), (1, 2));
    assert_eq!((recorte.pixels()[3], recorte.pixels()[7]), (128, 0));
    imagem.tons_de_cinza();
    imagem.ajustar_luz(0.1, 0.2);
    assert_eq!(imagem.pixels()[3], 128);
    let ampliada = imagem
        .redimensionar_vizinho(4, 4)
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(ampliada.dimensoes(), (4, 4));
    assert_eq!(ampliada.pixels()[3], 128);
    println!("OK imagem: RGBA, filtros, recorte, escala e alfa");
    Ok(())
}
