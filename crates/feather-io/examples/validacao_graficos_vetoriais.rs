//! CHT-013: gera SVG e PDF da mesma cena, sem depender de GUI.
use feather_charts::{Grafico, Serie, Tipo};
use feather_draw::Cor;
use std::{env, error::Error, fs, path::PathBuf};

fn demonstracao(tipo: Tipo) -> Grafico {
    let categorias = vec!["1".into(), "2".into(), "3".into(), "4".into()];
    let mut series = vec![Serie {
        nome: "Receita".into(),
        valores: vec![8.0, 16.0, 13.0, 24.0],
        cor: Cor::rgb(11, 87, 208),
    }];
    if tipo != Tipo::Pizza {
        series.push(Serie {
            nome: "Meta".into(),
            valores: vec![12.0, 14.0, 17.0, 20.0],
            cor: Cor::rgb(232, 113, 10),
        });
    }
    Grafico {
        titulo: "Validação vetorial".into(),
        tipo,
        categorias,
        series,
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let destino = PathBuf::from(env::args().nth(1).unwrap_or_else(|| {
        "/tmp/featheroffice-graficos".to_owned()
    }));
    fs::create_dir_all(&destino)?;

    for (nome, tipo) in [
        ("colunas", Tipo::Colunas),
        ("barras", Tipo::Barras),
        ("linhas", Tipo::Linhas),
        ("pizza", Tipo::Pizza),
        ("dispersao", Tipo::Dispersao),
    ] {
        let grafico = demonstracao(tipo);
        let svg = grafico.exportar_svg(720.0, 480.0)
            .map_err(|e| format!("SVG {nome}: {e:?}"))?;
        let pdf = grafico.exportar_pdf(720.0, 480.0)
            .map_err(|e| format!("PDF {nome}: {e:?}"))?;
        assert!(svg.contains("<svg"));
        assert!(pdf.starts_with(b"%PDF-1.4"));
        let svg_path = destino.join(format!("{nome}.svg"));
        let pdf_path = destino.join(format!("{nome}.pdf"));
        fs::write(&svg_path, svg)?;
        fs::write(&pdf_path, pdf)?;
        println!("OK {nome}: {} | {}", svg_path.display(), pdf_path.display());
    }
    Ok(())
}
