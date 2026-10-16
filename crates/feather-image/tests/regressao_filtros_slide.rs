//! Referências calculadas com as fórmulas de `featherslide-gui/src/imagem_processamento.rs`
//! anteriores à extração (tolerância de 2 LSB para diferenças f32/f64).

use feather_image::{aplicar_cores_rgba, AjustesCor, ImagemRgba};

fn conferir_proximo(real: &[u8], esperado: &[u8]) {
    for (indice, (&obtido, &anterior)) in real.iter().zip(esperado).enumerate() {
        assert!(
            obtido.abs_diff(anterior) <= 2,
            "canal {indice}: esperado {anterior}, obtido {obtido}"
        );
    }
}

#[test]
fn filtros_1_a_5_preservam_paleta_do_slide() {
    let amostras = [
        (1u8, [79, 79, 79]),
        (2u8, [111, 99, 77]),
        (3u8, [135, 221, 55]),
        (4u8, [128, 64, 191]),
        (5u8, [120, 34, 55]),
    ];
    for (filtro, esperado) in amostras {
        let mut pixels = [120, 34, 200, 128, 9, 8, 7, 0];
        aplicar_cores_rgba(
            &mut pixels,
            AjustesCor {
                filtro,
                ..AjustesCor::default()
            },
        )
        .unwrap();
        conferir_proximo(&pixels[..3], &esperado);
        assert_eq!(pixels[3], 128, "alfa original do pixel visível");
        assert_eq!(&pixels[4..], &[9, 8, 7, 0], "RGB invisível não muda");
    }
}

#[test]
fn cor_composta_matiz_temperatura_brilho_saturacao_contraste() {
    let mut pixels = [120, 34, 200, 255];
    aplicar_cores_rgba(
        &mut pixels,
        AjustesCor {
            brilho: 0.2,
            contraste: 0.15,
            saturacao: 1.3,
            temperatura: 0.4,
            matiz: 42.0,
            filtro: 2,
        },
    )
    .unwrap();
    conferir_proximo(&pixels[..3], &[135, 120, 94]);
    assert_eq!(pixels[3], 255);
}

#[test]
fn blur_mediana_e_nitidez_preservam_alfa() {
    let im = ImagemRgba::nova(3, 1, vec![0, 0, 0, 0, 255, 255, 255, 128, 0, 0, 0, 255]).unwrap();
    for transformada in [im.desfocar(1), im.reduzir_ruido(1)] {
        assert_eq!(
            [
                transformada.pixels()[3],
                transformada.pixels()[7],
                transformada.pixels()[11]
            ],
            [0, 128, 255]
        );
    }
    let mut nitida = im.clone();
    nitida.realcar_detalhes(0.5).unwrap();
    assert_eq!(
        [nitida.pixels()[3], nitida.pixels()[7], nitida.pixels()[11]],
        [0, 128, 255]
    );
}
