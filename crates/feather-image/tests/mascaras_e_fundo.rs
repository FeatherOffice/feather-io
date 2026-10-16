//! IMG-007: pixel-tests determinísticos de máscaras e recorte imutável.
use feather_image::{ErroMascara, ImagemRgba, MascaraAlfa};

fn uniforme(largura: u32, altura: u32, rgba: [u8; 4]) -> ImagemRgba {
    let pixels = (0..largura as usize * altura as usize)
        .flat_map(|_| rgba)
        .collect::<Vec<u8>>();
    ImagemRgba::nova(largura, altura, pixels).unwrap()
}

fn pos(w: usize, x: usize, y: usize) -> usize {
    y * w + x
}

#[test]
fn recorte_retangular_duro_nao_muda_a_origem() {
    let origem = uniforme(4, 2, [11, 22, 33, 255]);
    let mascara = MascaraAlfa::retangular(4, 2, 1.0, 0.0, 2.0, 2.0, 0.0).unwrap();
    assert_eq!(mascara.alfa(), &[0, 255, 255, 0, 0, 255, 255, 0]);
    let saida = mascara.aplicar(&origem).unwrap();
    assert_eq!(
        saida.pixels().chunks_exact(4).map(|p| p[3]).collect::<Vec<_>>(),
        mascara.alfa().to_vec()
    );
    assert_eq!(origem, uniforme(4, 2, [11, 22, 33, 255]));
    assert!(saida.pixels().chunks_exact(4).all(|p| p[..3] == [11, 22, 33]));
}

#[test]
fn corte_suave_tem_cobertura_gradual_sem_alterar_rgb() {
    let origem = uniforme(9, 9, [101, 30, 220, 255]);
    let m = MascaraAlfa::retangular(9, 9, 2.0, 2.0, 5.0, 5.0, 2.0).unwrap();
    let y = 4;
    let a = m.alfa();
    assert!(0 < a[pos(9, 0, y)]);
    assert!(a[pos(9, 0, y)] < a[pos(9, 1, y)]);
    assert!(a[pos(9, 1, y)] < a[pos(9, 2, y)]);
    assert_eq!(a[pos(9, 4, y)], 255);
    assert_eq!(a[pos(9, 8, y)], a[pos(9, 0, y)]);
    let vista = m.aplicar(&origem).unwrap();
    assert_eq!(origem.pixels()[3], 255);
    assert_eq!(vista.pixels()[3], a[0]);
    assert!(vista.pixels().chunks_exact(4).all(|p| p[..3] == [101, 30, 220]));
}

#[test]
fn remocao_de_fundo_respeita_ilha_interna_da_mesma_cor() {
    let mut dados = uniforme(5, 5, [20, 30, 40, 255]).em_pixels();
    for y in 1..=3 {
        for x in 1..=3 {
            let idx = pos(5, x, y) * 4;
            dados[idx..idx + 4].copy_from_slice(&[200, 10, 20, 255]);
        }
    }
    // A ilha central corresponde ao fundo, mas é cercada por cor diferente.
    dados[pos(5, 2, 2) * 4..pos(5, 2, 2) * 4 + 4]
        .copy_from_slice(&[20, 30, 40, 255]);
    let origem = ImagemRgba::nova(5, 5, dados).unwrap();
    let mascara = MascaraAlfa::fundo_por_cor(&origem, [20, 30, 40], 0, 0);
    assert_eq!(mascara.alfa()[pos(5, 0, 0)], 0);
    assert_eq!(mascara.alfa()[pos(5, 1, 1)], 255);
    assert_eq!(mascara.alfa()[pos(5, 2, 2)], 255);
    assert_eq!(mascara.aplicar(&origem).unwrap().pixels()[3], 0);
    assert_eq!(origem.pixels()[3], 255);
}

#[test]
fn anti_alias_de_remocao_usa_tolerancia_e_faixa_controlada() {
    let origem = ImagemRgba::nova(
        3,
        1,
        vec![10, 10, 10, 255, 13, 13, 13, 255, 70, 70, 70, 255],
    ).unwrap();
    let m = MascaraAlfa::fundo_por_cor(&origem, [10, 10, 10], 2, 2);
    assert_eq!(m.alfa(), &[0, 128, 255]);
    assert_eq!(
        m.aplicar(&origem).unwrap().pixels()
            .chunks_exact(4).map(|pixel| pixel[3]).collect::<Vec<_>>(),
        vec![0, 128, 255]
    );
}

#[test]
fn fundo_inexistente_na_borda_nao_apaga_objetos_do_interior() {
    let origem = uniforme(4, 3, [100, 50, 10, 128]);
    let m = MascaraAlfa::fundo_por_cor(&origem, [0, 0, 0], 0, 0);
    assert!(m.alfa().iter().all(|&a| a == 255));
    assert_eq!(m.aplicar(&origem).unwrap(), origem);
}

#[test]
fn mascara_multiplica_alfa_antigo_sem_afetar_rgb() {
    let origem = ImagemRgba::nova(
        3, 1, vec![1, 2, 3, 0, 4, 5, 6, 128, 7, 8, 9, 255]
    ).unwrap();
    let m = MascaraAlfa::nova(3, 1, vec![255, 128, 0]).unwrap();
    let p = m.aplicar(&origem).unwrap().em_pixels();
    assert_eq!(p, vec![1, 2, 3, 0, 4, 5, 6, 64, 7, 8, 9, 0]);
    assert_eq!(origem.pixels()[7], 128);
}

#[test]
fn composicao_de_mascaras_e_valida_dimensoes_sem_mutar() {
    let a = MascaraAlfa::nova(3, 1, vec![255, 128, 0]).unwrap();
    let b = MascaraAlfa::nova(3, 1, vec![100, 128, 200]).unwrap();
    let c = a.combinar(&b).unwrap();
    assert_eq!(c.alfa(), &[100, 64, 0]);
    assert_eq!(a.alfa(), &[255, 128, 0]);
    assert_eq!(b.alfa(), &[100, 128, 200]);
    let divergente = MascaraAlfa::nova(1, 1, vec![255]).unwrap();
    assert_eq!(a.combinar(&divergente), Err(ErroMascara::DimensoesDiferentes));
    assert_eq!(
        a.aplicar(&uniforme(1, 1, [1, 2, 3, 4])),
        Err(ErroMascara::DimensoesDiferentes)
    );
}

#[test]
fn rejeita_buffer_area_e_raio_invalidos() {
    assert_eq!(MascaraAlfa::nova(0, 1, vec![]), Err(ErroMascara::DimensoesInvalidas));
    assert_eq!(MascaraAlfa::nova(2, 1, vec![255]), Err(ErroMascara::BufferInvalido));
    assert_eq!(
        MascaraAlfa::nova(u32::MAX, u32::MAX, vec![]),
        Err(ErroMascara::DimensoesInvalidas)
    );
    assert_eq!(
        MascaraAlfa::retangular(2, 2, 0., 0., f32::NAN, 1., 0.),
        Err(ErroMascara::AreaInvalida)
    );
    assert_eq!(
        MascaraAlfa::retangular(2, 2, 0., 0., 1., 1., f32::INFINITY),
        Err(ErroMascara::SuavizacaoInvalida)
    );
    assert_eq!(
        MascaraAlfa::retangular(2, 2, 0., 0., 1., 0., 0.),
        Err(ErroMascara::AreaInvalida)
    );
}

#[test]
fn intersecao_de_recortes_reduz_area_visivel() {
    let esquerda = MascaraAlfa::retangular(4, 2, 0., 0., 3., 2., 0.).unwrap();
    let direita = MascaraAlfa::retangular(4, 2, 2., 0., 2., 2., 0.).unwrap();
    let m = esquerda.combinar(&direita).unwrap();
    assert_eq!(m.alfa(), &[0, 0, 255, 0, 0, 0, 255, 0]);
}

#[test]
fn alfa_aleatorio_nunca_aumenta_e_origem_permanece_imutavel() {
    let mut estado: u64 = 0xF31A_007A_E2D6_1E51;
    let mut proximo = || {
        estado ^= estado << 13;
        estado ^= estado >> 7;
        estado ^= estado << 17;
        (estado >> 32) as u8
    };
    for _caso in 0..128 {
        let (w, h) = (1 + u32::from(proximo() % 12), 1 + u32::from(proximo() % 12));
        let mut dados = Vec::new();
        let mut alfas = Vec::new();
        for _ in 0..w * h {
            dados.extend([proximo(), proximo(), proximo(), proximo()]);
            alfas.push(proximo());
        }
        let origem = ImagemRgba::nova(w, h, dados.clone()).unwrap();
        let mascara = MascaraAlfa::nova(w, h, alfas).unwrap();
        let saida = mascara.aplicar(&origem).unwrap();
        for (a, b) in dados.chunks_exact(4).zip(saida.pixels().chunks_exact(4)) {
            assert_eq!(&a[..3], &b[..3]);
            assert!(b[3] <= a[3]);
        }
        assert_eq!(origem.pixels(), dados);
    }
}
