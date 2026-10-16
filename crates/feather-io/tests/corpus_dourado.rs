//! QA-003: corpus dourado versionado de tabelas, imagens e gráficos.
//! Os valores de referência não são atualizados automaticamente.
use feather_charts::{Grafico, Serie, Tipo};
use feather_draw::{Cor, Figura};
use feather_image::ImagemRgba;
use feather_io::{Documento, Registro};
use std::fmt::Write as _;

#[test]
fn documento_csv_conserva_unicode_multilinha_e_bytes_canonicos() {
    let entrada = include_bytes!("../../../fixtures/golden/documento-entrada.csv");
    let esperado = include_bytes!("../../../fixtures/golden/documento-esperado.csv");
    let registro = Registro::padrao();
    let documento = registro.ler("csv", entrada).unwrap();
    let Documento::Tabela(tabela) = &documento else {
        panic!("a fixture de documento deve ser tabular");
    };
    assert_eq!(tabela.dimensoes(), (3, 3));
    assert_eq!(tabela.obter(1, 1).unwrap().valor.texto(), "Linha 1\nLinha 2");
    assert_eq!(tabela.obter(2, 0).unwrap().valor.texto(), "Anel \"A\"");
    let serializado = registro.gravar("csv", &documento).unwrap();
    assert_eq!(serializado, esperado, "regressão byte-a-byte do CSV");
    assert_eq!(registro.ler("csv", esperado).unwrap(), documento);
}

#[test]
fn imagem_rgba_preserva_alpha_e_pixeis_esperados() {
    let fonte = include_str!("../../../fixtures/golden/imagem-entrada.rgba.txt");
    let valores: Vec<u32> = fonte.split_whitespace()
        .map(|v| v.parse::<u32>().unwrap()).collect();
    let (largura, altura) = (valores[0], valores[1]);
    let bytes = valores[2..].iter().map(|&v| u8::try_from(v).unwrap()).collect();
    let mut imagem = ImagemRgba::nova(largura, altura, bytes).unwrap();
    imagem.espelhar_horizontal();
    imagem.espelhar_vertical();
    let mut saida = format!("{largura} {altura}\n");
    for px in imagem.pixels().chunks_exact(4) {
        writeln!(saida, "{} {} {} {}", px[0], px[1], px[2], px[3]).unwrap();
    }
    assert_eq!(
        saida,
        include_str!("../../../fixtures/golden/imagem-esperada.rgba.txt")
    );
}

#[test]
fn grafico_serializa_resumo_semantico_e_quatro_barras() {
    let fonte = include_str!("../../../fixtures/golden/grafico-entrada.csv");
    let mut linhas = fonte.lines();
    let cabecalho: Vec<_> = linhas.next().unwrap().split(',').collect();
    assert_eq!(cabecalho.len(), 3);
    let mut categorias = Vec::new();
    let mut v1 = Vec::new();
    let mut v2 = Vec::new();
    for linha in linhas {
        let campos: Vec<_> = linha.split(',').collect();
        assert_eq!(campos.len(), 3);
        categorias.push(campos[0].to_string());
        v1.push(campos[1].parse::<f64>().unwrap());
        v2.push(campos[2].parse::<f64>().unwrap());
    }
    let grafico = Grafico {
        titulo: String::new(),
        tipo: Tipo::Colunas,
        categorias,
        series: vec![
            Serie { nome: cabecalho[1].into(), valores: v1, cor: Cor::rgb(11, 87, 208) },
            Serie { nome: cabecalho[2].into(), valores: v2, cor: Cor::rgb(232, 113, 10) },
        ],
    };
    let cena = grafico.cena(640.0, 360.0).unwrap();
    let barras = cena.figuras.iter()
        .filter(|forma| matches!(forma, Figura::Retangulo { .. })).count();
    let valores = |s: &Serie| s.valores.iter().map(|n| n.to_string())
        .collect::<Vec<_>>().join(",");
    let resumo = format!(
        "tipo=colunas\ncategorias={}\nseries={}\nvalores={}|{}\nbarras={}\n",
        grafico.categorias.join("|"),
        grafico.series.iter().map(|s| s.nome.as_str()).collect::<Vec<_>>().join("|"),
        valores(&grafico.series[0]), valores(&grafico.series[1]), barras
    );
    assert_eq!(resumo, include_str!("../../../fixtures/golden/grafico-esperado.txt"));
    assert_eq!(cena, grafico.cena(640.0, 360.0).unwrap(), "Cena determinística");
}
