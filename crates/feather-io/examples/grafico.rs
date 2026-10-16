//! Fixture tabular -> gráficos com séries compartilhadas no Drawing.
use feather_charts::{Grafico, Serie, Tipo};
use feather_draw::{Cor, Figura};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut categorias = Vec::new();
    let mut vendas = Vec::new();
    let mut meta = Vec::new();
    for linha in include_str!("../../../fixtures/grafico.csv")
        .lines()
        .skip(1)
    {
        let colunas: Vec<_> = linha.split(',').collect();
        assert_eq!(colunas.len(), 3);
        categorias.push(colunas[0].to_owned());
        vendas.push(colunas[1].parse::<f64>()?);
        meta.push(colunas[2].parse::<f64>()?);
    }
    let grafico = Grafico {
        titulo: "Vendas x meta".into(),
        tipo: Tipo::Colunas,
        categorias,
        series: vec![
            Serie {
                nome: "Vendas".into(),
                valores: vendas,
                cor: Cor::rgb(12, 90, 180),
            },
            Serie {
                nome: "Meta".into(),
                valores: meta,
                cor: Cor::rgb(240, 160, 24),
            },
        ],
    };
    let colunas = grafico.cena(640.0, 360.0).map_err(|e| format!("{e:?}"))?;
    assert_eq!(colunas.figuras.len(), 7, "eixos + seis barras");
    assert_eq!(
        colunas
            .figuras
            .iter()
            .filter(|f| matches!(f, Figura::Retangulo { .. }))
            .count(),
        6
    );
    let linhas = Grafico {
        tipo: Tipo::Linhas,
        ..grafico.clone()
    }
    .cena(640.0, 360.0)
    .map_err(|e| format!("{e:?}"))?;
    assert_eq!(linhas.figuras.len(), 3, "eixos + duas linhas");
    println!("OK gráficos: duas séries em barras e linhas");
    Ok(())
}
