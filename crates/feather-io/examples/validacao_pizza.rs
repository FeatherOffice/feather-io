//! Fixture executável para aceitar a tipografia real de pizza em SVG.
use feather_charts::{Grafico, Serie, Tipo};
use feather_draw::Cor;
fn main() {
    let g = Grafico {
        titulo: "Aceite de rótulos".into(),
        tipo: Tipo::Pizza,
        categorias: vec![
            "Alpha & Omega".into(),
            "WWWiii Ação".into(),
            "<Texto>".into(),
            "日本語".into(),
        ],
        series: vec![Serie {
            nome: "Dados".into(),
            valores: vec![1., 2., 3., 4.],
            cor: Cor::rgb(0, 0, 255),
        }],
    };
    let svg = g.exportar_svg(640., 320.).unwrap();
    assert_eq!(svg.matches("<text ").count(), 4);
    print!("{svg}");
}
