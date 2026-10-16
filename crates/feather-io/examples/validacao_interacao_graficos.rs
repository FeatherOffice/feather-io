//! CHT-012: simula mouse Iced e edição de uma amostra em cinco gráficos.
//! Rodar: cargo run -p feather-io
//!           --example validacao_interacao_graficos -j 2
use feather_charts::{
    AparenciaGrafico, EventoGrafico, Grafico, InteracaoGrafico, Serie, Tipo,
    ViewportGrafico,
};
use feather_draw::{Cor, Figura, Ponto};
use std::error::Error;

fn exemplo(tipo: Tipo) -> Grafico {
    Grafico {
        titulo: "Inspeção de série".into(),
        tipo,
        categorias: vec!["0".into(), "1".into(), "2".into()],
        series: vec![Serie {
            nome: "A".into(),
            valores: vec![10.0, 20.0, 30.0],
            cor: Cor::rgb(11, 87, 208),
        }],
    }
}

fn ponto_visivel(g: &Grafico) -> Ponto {
    let cena = g.cena(600.0, 400.0).expect("cena");
    match g.tipo {
        Tipo::Colunas | Tipo::Barras => {
            let area = cena.figuras.iter().find_map(|f| match f {
                Figura::Retangulo { area, .. } => Some(*area),
                _ => None,
            }).expect("primeira barra");
            Ponto::novo(area.x + area.largura * 0.5, area.y + area.altura * 0.5)
        }
        Tipo::Linhas | Tipo::Area | Tipo::Degraus | Tipo::Dispersao => {
            let eixo = g.eixos(600.0, 400.0).unwrap().unwrap();
            Ponto::novo(
                eixo.area.x,
                eixo.area.y + eixo.area.altura
                    * (1.0 - eixo.escala_y.unwrap().fracao(g.series[0].valores[0])),
            )
        }
        Tipo::Pizza => {
            let fatia = &g.fatias_pizza(600.0, 400.0).unwrap()[0];
            let angulo = (fatia.inicio + fatia.fim) * 0.5;
            Ponto::novo(300.0 + angulo.cos() as f32 * 90.0,
                        200.0 + angulo.sin() as f32 * 90.0)
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let aparencia = AparenciaGrafico::default();
    // Simula Canvas Iced deslocado 100x50 na janela e ampliado 2x.
    let viewport = ViewportGrafico { x: 100.0, y: 50.0, largura: 1200.0, altura: 800.0 };
    for tipo in [
        Tipo::Colunas, Tipo::Barras, Tipo::Linhas, Tipo::Dispersao, Tipo::Pizza,
    ] {
        let mut grafico = exemplo(tipo);
        let p = ponto_visivel(&grafico);
        let tela = Ponto::novo(100.0 + p.x * 2.0, 50.0 + p.y * 2.0);
        let mut estado = InteracaoGrafico::default();
        let hover = estado.processar(
            EventoGrafico::Mover(tela), &grafico, 600.0, 400.0,
            viewport, 5.0, &aparencia,
        ).map_err(|e| format!("hover {tipo:?}: {e:?}"))?;
        assert!(hover.hover);
        let clique = estado.processar(
            EventoGrafico::Clicar(tela), &grafico, 600.0, 400.0,
            viewport, 5.0, &aparencia,
        ).map_err(|e| format!("click {tipo:?}: {e:?}"))?;
        assert!(clique.selecao);
        let antigo = estado.selecionado.as_ref().unwrap().valor;
        let anterior = estado.editar_selecionado(&mut grafico, antigo + 2.0)
            .map_err(|e| format!("edit {tipo:?}: {e:?}"))?;
        assert_eq!(anterior, antigo);
        assert_eq!(grafico.series[0].valores[0], antigo + 2.0);
        println!("OK {tipo:?}: hover, seleção, edição {antigo} -> {}", antigo + 2.0);
    }
    Ok(())
}
