//! Fixture de formas para uma Cena independente de Iced.
use feather_draw::{transformar, Cena, Estilo, Figura, Ponto, Retangulo, Transformacao};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut cena = Cena::nova(320.0, 200.0);
    for linha in include_str!("../../../fixtures/cena.csv").lines().skip(1) {
        let cols: Vec<_> = linha.split(',').collect();
        assert_eq!(cols.len(), 5);
        assert_eq!(cols[0], "retangulo");
        let valores: Vec<f32> = cols[1..]
            .iter()
            .map(|s| s.parse())
            .collect::<Result<_, _>>()?;
        cena.adicionar(Figura::Retangulo {
            area: Retangulo::novo(valores[0], valores[1], valores[2], valores[3]),
            estilo: Estilo::default(),
        });
    }
    assert_eq!(cena.figuras.len(), 2);
    match transformar(&cena.figuras[0], Transformacao::transladar(7.0, 9.0)) {
        Figura::Polilinha {
            pontos, fechada, ..
        } => {
            assert!(fechada);
            assert_eq!(pontos.len(), 4);
            assert_eq!(pontos[0], Ponto::novo(7.0, 9.0));
            assert_eq!(pontos[2], Ponto::novo(127.0, 69.0));
        }
        _ => panic!("retângulo transformado deve ser polilinha"),
    }
    println!(
        "OK cena: {} figuras em {}x{}",
        cena.figuras.len(),
        cena.largura,
        cena.altura
    );
    Ok(())
}
