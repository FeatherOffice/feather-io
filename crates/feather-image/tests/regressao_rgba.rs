//! Regressões de pixel, dimensões e buffers inválidos do motor RGBA.
//! PNG/JPEG/WebP são formatos tratados por adaptadores: o domínio recebe RGBA cru.

use feather_image::{ErroImagem, ImagemRgba};

fn alfas(imagem: &ImagemRgba) -> Vec<u8> {
    imagem
        .pixels()
        .chunks_exact(4)
        .map(|pixel| pixel[3])
        .collect()
}

#[test]
fn alfa_zero_meio_e_opaco_sobrevivem_a_luz_e_cinza() {
    let mut imagem = ImagemRgba::nova(
        3,
        1,
        vec![240, 10, 10, 0, 10, 240, 10, 128, 10, 10, 240, 255],
    )
    .unwrap();

    imagem.ajustar_luz(0.4, -0.3);
    imagem.tons_de_cinza();

    assert_eq!(alfas(&imagem), [0, 128, 255]);
    for pixel in imagem.pixels().chunks_exact(4) {
        assert_eq!(pixel[0], pixel[1]);
        assert_eq!(pixel[1], pixel[2]);
    }
}

#[test]
fn espelhamento_horizontal_e_vertical_preserva_alfa_em_grade_impar() {
    let pixels = (0u8..9).flat_map(|alfa| [10, 20, 30, alfa]).collect();
    let mut imagem = ImagemRgba::nova(3, 3, pixels).unwrap();

    imagem.espelhar_horizontal();
    assert_eq!(alfas(&imagem), [2, 1, 0, 5, 4, 3, 8, 7, 6]);

    imagem.espelhar_vertical();
    assert_eq!(alfas(&imagem), [8, 7, 6, 5, 4, 3, 2, 1, 0]);
}

#[test]
fn redimensionamento_nearest_preserva_pixels_e_alfa_exatos() {
    let original = ImagemRgba::nova(
        2,
        2,
        vec![1, 2, 3, 0, 4, 5, 6, 128, 7, 8, 9, 255, 10, 11, 12, 64],
    )
    .unwrap();
    let ampliada = original.redimensionar_vizinho(4, 4).unwrap();
    assert_eq!(ampliada.dimensoes(), (4, 4));
    assert_eq!(
        alfas(&ampliada),
        [0, 0, 128, 128, 0, 0, 128, 128, 255, 255, 64, 64, 255, 255, 64, 64]
    );
    assert_eq!(&ampliada.pixels()[0..4], &[1, 2, 3, 0]);
    assert_eq!(&ampliada.pixels()[12..16], &[4, 5, 6, 128]);
    assert_eq!(&ampliada.pixels()[60..64], &[10, 11, 12, 64]);
    assert_eq!(original.dimensoes(), (2, 2));
}

#[test]
fn recorte_preserva_alfa_e_nao_altera_imagem_de_origem() {
    let imagem = ImagemRgba::nova(3, 1, vec![1, 2, 3, 0, 4, 5, 6, 128, 7, 8, 9, 255]).unwrap();
    let anterior = imagem.pixels().to_vec();

    let recortada = imagem.cortar(1, 0, 2, 1).unwrap();
    assert_eq!(recortada.dimensoes(), (2, 1));
    assert_eq!(recortada.pixels(), &[4, 5, 6, 128, 7, 8, 9, 255]);
    assert_eq!(imagem.pixels(), anterior);
}

#[test]
fn tamanho_minimo_um_por_um_e_permitido() {
    let mut imagem = ImagemRgba::nova(1, 1, vec![9, 40, 70, 128]).unwrap();
    imagem.espelhar_horizontal();
    imagem.espelhar_vertical();
    assert_eq!(imagem.pixels(), &[9, 40, 70, 128]);
    assert_eq!(
        imagem.redimensionar_vizinho(1, 1).unwrap().pixels(),
        imagem.pixels()
    );
}

#[test]
fn dimensoes_zero_sao_rejeitadas_na_criacao_e_resize() {
    for (largura, altura) in [(0, 0), (0, 1), (1, 0)] {
        assert_eq!(
            ImagemRgba::nova(largura, altura, vec![]),
            Err(ErroImagem::DimensoesInvalidas)
        );
    }
    let imagem = ImagemRgba::nova(1, 1, vec![1, 2, 3, 4]).unwrap();
    for (largura, altura) in [(0, 0), (0, 1), (1, 0)] {
        assert_eq!(
            imagem.redimensionar_vizinho(largura, altura),
            Err(ErroImagem::DimensoesInvalidas)
        );
    }
}

#[test]
fn dimensoes_extremas_falham_sem_tentar_alocar() {
    assert_eq!(
        ImagemRgba::nova(u32::MAX, u32::MAX, vec![]),
        Err(ErroImagem::DimensoesInvalidas)
    );
    let imagem = ImagemRgba::nova(1, 1, vec![0, 0, 0, 255]).unwrap();
    assert_eq!(
        imagem.redimensionar_vizinho(u32::MAX, u32::MAX),
        Err(ErroImagem::DimensoesInvalidas)
    );
}

#[test]
fn comprimento_de_buffer_rgba_invalido_e_rejeitado() {
    // O buffer RGBA cru precisa ter exatamente 4 * largura * altura bytes.
    for tamanho in [0, 1, 3, 5, 7, 9] {
        assert_eq!(
            ImagemRgba::nova(2, 1, vec![0; tamanho]),
            Err(ErroImagem::PixelsInvalidos)
        );
    }
    assert_eq!(
        ImagemRgba::nova(2, 1, vec![0; 8]).unwrap().dimensoes(),
        (2, 1)
    );
}

#[test]
fn recortes_fora_de_limite_e_overflow_sao_rejeitados() {
    let imagem = ImagemRgba::nova(2, 2, vec![0; 16]).unwrap();
    for (x, y, largura, altura) in [
        (0, 0, 0, 1),
        (0, 0, 1, 0),
        (2, 0, 1, 1),
        (0, 2, 1, 1),
        (1, 1, 2, 1),
        (1, 1, 1, 2),
        (u32::MAX, 0, 2, 1),
        (0, u32::MAX, 1, 2),
    ] {
        assert_eq!(
            imagem.cortar(x, y, largura, altura),
            Err(ErroImagem::RecorteInvalido)
        );
    }
}

#[test]
fn valores_nao_finitos_de_luz_nao_apagam_alfa() {
    let mut imagem = ImagemRgba::nova(2, 1, vec![0, 255, 0, 0, 255, 0, 255, 255]).unwrap();
    let anterior = imagem.pixels().to_vec();

    imagem.ajustar_luz(f32::NAN, f32::INFINITY);
    assert_eq!(imagem.pixels(), anterior);
    imagem.ajustar_luz(f32::NEG_INFINITY, f32::NAN);
    assert_eq!(alfas(&imagem), [0, 255]);
}
