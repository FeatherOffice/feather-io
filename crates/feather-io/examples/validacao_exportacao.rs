//! Smoke manual da IO-011: três saídas reais com os codecs comuns.
//! cargo run -p feather-io --example validacao_exportacao -- /tmp/featheroffice-export
use feather_draw::{Cena, Cor, Estilo, Figura, Retangulo};
use feather_image::ImagemRgba;
use feather_io::{Documento, Registro};
use std::{env, error::Error, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
    let destino = PathBuf::from(env::args().nth(1).unwrap_or_else(|| "/tmp/featheroffice-export".into()));
    fs::create_dir_all(&destino)?;

    let mut cena = Cena::nova(240.0, 150.0);
    cena.adicionar(Figura::Retangulo {
        area: Retangulo::novo(30.0, 20.0, 120.0, 90.0),
        estilo: Estilo {
            preenchimento: Some(Cor::rgba(15, 112, 201, 180)),
            contorno: Some(Cor::rgb(35, 55, 75)),
            espessura: 2.0,
        },
    });
    let desenho = Documento::Desenho(cena);
    let imagem = Documento::Imagem(ImagemRgba::nova(
        2, 2,
        vec![
            255, 0, 0, 0, 0, 255, 0, 128,
            0, 0, 255, 255, 255, 255, 255, 255,
        ],
    ).map_err(|e| format!("ImagemRgba inválida: {e:?}"))?);
    let registro = Registro::padrao();

    for (nome, formato, doc) in [
        ("exemplo.svg", "svg", &desenho),
        ("exemplo.pdf", "pdf", &desenho),
        ("exemplo.png", "png", &imagem),
    ] {
        let dados = registro.gravar(formato, doc)
            .map_err(|e| format!("{formato}: {e:?}"))?;
        let caminho = destino.join(nome);
        fs::write(&caminho, &dados)?;
        println!("{} — {} bytes", caminho.display(), dados.len());
    }
    Ok(())
}
