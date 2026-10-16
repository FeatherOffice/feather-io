//! Exemplo executável CHT-007: exporta SVG/PDF anotados nos dois locales.
use feather_charts::{FormatoNumero, Grafico, LocalidadeGrafico, Serie, TipografiaGrafico, Tipo};
use feather_draw::Cor;
use std::{env, error::Error, fs, path::PathBuf};

fn grafico(tipo: Tipo) -> Grafico {
    let mut series = vec![Serie {
        nome: "Receita".into(), valores: vec![1200.0, 2300.0, 1500.0],
        cor: Cor::rgb(43, 122, 120),
    }];
    if tipo != Tipo::Pizza {
        series.push(Serie {
            nome: "Custos".into(), valores: vec![800.0, 900.0, 600.0],
            cor: Cor::rgb(232, 113, 10),
        });
    }
    Grafico {
        titulo: "Receita & custos".into(), tipo,
        categorias: vec!["1".into(), "2".into(), "3".into()], series,
    }
}
fn main() -> Result<(), Box<dyn Error>> {
    let saida = PathBuf::from(env::args().nth(1).unwrap_or_else(|| "/tmp/featheroffice-cht007".into()));
    fs::create_dir_all(&saida)?;
    for (nome, tipo) in [
        ("colunas", Tipo::Colunas), ("barras", Tipo::Barras),
        ("linhas", Tipo::Linhas), ("pizza", Tipo::Pizza),
        ("dispersao", Tipo::Dispersao),
    ] {
        let g = grafico(tipo);
        for (idioma, localidade) in [
            ("ptbr", LocalidadeGrafico::PtBr), ("enus", LocalidadeGrafico::EnUs),
        ] {
            let cfg = TipografiaGrafico {
                subtitulo: if idioma == "ptbr" {
                    "Relatório mensal 2026".into()
                } else {
                    "Monthly report 2026".into()
                },
                unidade: "MWh".into(),
                localidade,
                formato: FormatoNumero::Moeda(2),
                mostrar_valores: matches!(tipo, Tipo::Colunas | Tipo::Barras),
                ..Default::default()
            };
            let svg = g.exportar_svg_anotado(800.0, 520.0, &cfg)
                .map_err(|e| format!("svg {nome}/{idioma}: {e:?}"))?;
            let pdf = g.exportar_pdf_anotado(800.0, 520.0, &cfg)
                .map_err(|e| format!("pdf {nome}/{idioma}: {e:?}"))?;
            assert!(svg.contains("Receita &amp; custos"));
            assert!(pdf.starts_with(b"%PDF-1.4"));
            let base = format!("{nome}_{idioma}");
            fs::write(saida.join(format!("{base}.svg")), svg)?;
            fs::write(saida.join(format!("{base}.pdf")), pdf)?;
            println!("OK {base}.svg | {base}.pdf");
        }
    }
    println!("20 arquivos em {}", saida.display());
    Ok(())
}
