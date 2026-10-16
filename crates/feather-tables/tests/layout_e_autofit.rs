//! Regressões de TBL-008: layout independente de GUI e tabelas esparsas.
use feather_tables::{
    Celula, ConfigLayout, ErroLayout, EstiloCelula, EstiloLayout, LayoutTabela,
    MedidaTexto, MedidorMonoespacado, Regiao, Tabela, Valor,
};

fn medir() -> MedidorMonoespacado {
    MedidorMonoespacado {
        largura_caractere: 5.0,
        altura_linha: 10.0,
    }
}
fn celula(texto: &str, quebra: bool) -> Celula {
    Celula {
        valor: Valor::Texto(texto.into()),
        estilo: EstiloCelula {
            quebrar_linha: quebra,
            ..Default::default()
        },
    }
}

#[test]
fn quebras_respeitam_palavras_multilinhas_e_a_celula_original() {
    let mut tabela = Tabela::nova(2, 1);
    tabela.definir_largura(0, 15.0).unwrap();
    tabela.definir(0, 0, celula("AB CD\nEF", true)).unwrap();
    let antes = tabela.clone();
    let layout = LayoutTabela::default();
    let resultado = layout.medir_celula(&tabela, 0, 0, &medir()).unwrap();
    assert_eq!(resultado.linhas, vec!["AB", "CD", "EF"]);
    assert_eq!(resultado.altura, 30.0);
    assert_eq!(resultado.largura, 10.0);
    assert_eq!(tabela, antes);
}

#[test]
fn quebra_utf8_nunca_corta_bytes_dentro_de_caracteres() {
    let mut tabela = Tabela::nova(1, 1);
    tabela.definir_largura(0, 10.0).unwrap();
    tabela.definir(0, 0, celula("ação🙂", true)).unwrap();
    let linhas = LayoutTabela::default()
        .medir_celula(&tabela, 0, 0, &medir())
        .unwrap()
        .linhas;
    assert_eq!(linhas, vec!["aç", "ão", "🙂"]);
}

#[test]
fn auto_fit_largura_usa_maior_paragrafo_e_padding_sem_perder_estilo() {
    let mut tabela = Tabela::nova(100, 4);
    tabela.definir(1, 2, celula("abcd\nbb", true)).unwrap();
    let mut layout_celula = EstiloLayout::default();
    layout_celula.padding.esquerda = 2.0;
    layout_celula.padding.direita = 3.0;
    tabela.definir_layout(1, 2, layout_celula.clone()).unwrap();
    tabela.definir(99, 2, celula("x", false)).unwrap();
    let layout = LayoutTabela::default();
    assert_eq!(layout.largura_auto(&tabela, 2, &medir()), Ok(25.0));
    assert_eq!(layout.auto_ajustar_coluna(&mut tabela, 2, &medir()), Ok(25.0));
    assert_eq!(tabela.largura(2), Some(25.0));
    assert_eq!(tabela.layout(1, 2), Some(&layout_celula));
    assert_eq!(tabela.obter(1, 2).unwrap().valor, Valor::Texto("abcd\nbb".into()));
}

#[test]
fn auto_fit_altura_leva_em_conta_wrap_padding_e_entrelinhas() {
    let mut tabela = Tabela::nova(1, 1);
    tabela.definir(0, 0, celula("AB CD", true)).unwrap();
    tabela.definir_largura(0, 15.0).unwrap();
    let mut estilo = EstiloLayout::default();
    estilo.entre_linhas = 1.5;
    estilo.padding.cima = 3.0;
    estilo.padding.baixo = 2.0;
    tabela.definir_layout(0, 0, estilo).unwrap();
    let layout = LayoutTabela::default();
    assert_eq!(layout.altura_auto(&tabela, 0, &medir()), Ok(35.0));
    assert_eq!(layout.auto_ajustar_linha(&mut tabela, 0, &medir()), Ok(35.0));
    assert_eq!(tabela.altura(0), Some(35.0));
}

#[test]
fn ocultar_linha_coluna_preserva_armazenamento_e_posicoes_logicas() {
    let mut tabela = Tabela::nova(1_000_000, 1_000_000);
    tabela.definir(500_001, 600_001, celula("preservar", false)).unwrap();
    tabela.definir_altura(500_002, 40.0).unwrap();
    tabela.definir_largura(600_002, 30.0).unwrap();
    let snapshot = tabela.clone();
    let mut layout = LayoutTabela::default();
    layout.ocultar_linha(&tabela, 500_001, true).unwrap();
    layout.ocultar_coluna(&tabela, 600_001, true).unwrap();
    assert_eq!(layout.altura(&tabela, 500_001), Ok(0.0));
    assert_eq!(layout.largura(&tabela, 600_001), Ok(0.0));
    let linhas = layout.linhas_visiveis(&tabela, 500_000, 500_003).unwrap();
    assert_eq!(linhas.len(), 2);
    assert_eq!((linhas[0].indice, linhas[0].origem, linhas[0].tamanho), (500_000, 0.0, 24.0));
    assert_eq!((linhas[1].indice, linhas[1].origem, linhas[1].tamanho), (500_002, 24.0, 40.0));
    let colunas = layout.colunas_visiveis(&tabela, 600_000, 600_003).unwrap();
    assert_eq!(colunas.iter().map(|c| c.indice).collect::<Vec<_>>(), [600_000, 600_002]);
    assert_eq!(colunas[1].origem, 96.0);
    assert_eq!(colunas[1].tamanho, 30.0);
    assert_eq!(tabela, snapshot);
    layout.ocultar_linha(&tabela, 500_001, false).unwrap();
    layout.ocultar_coluna(&tabela, 600_001, false).unwrap();
    assert_eq!(layout.altura(&tabela, 500_001), Ok(24.0));
    assert_eq!(layout.largura(&tabela, 600_001), Ok(96.0));
}

#[test]
fn merge_nao_distorce_auto_fit_de_coluna_ou_linha_individual() {
    let mut tabela = Tabela::nova(4, 4);
    tabela.definir(0, 0, celula("ABCDEFGHIJ", true)).unwrap();
    tabela.mesclar(Regiao { linha: 0, coluna: 0, linhas: 1, colunas: 2 }).unwrap();
    tabela.definir_largura(0, 20.0).unwrap();
    tabela.definir_largura(1, 20.0).unwrap();
    let layout = LayoutTabela::default();
    assert_eq!(layout.largura_auto(&tabela, 0, &medir()), Ok(16.0));
    assert_eq!(
        layout.medir_celula(&tabela, 0, 0, &medir()).unwrap().linhas,
        vec!["ABCDEFGH", "IJ"]
    );
    assert_eq!(layout.altura_auto(&tabela, 0, &medir()), Ok(20.0));
}

#[test]
fn medida_customizada_pode_usar_negrito_e_fonte_do_consumidor() {
    let mut tabela = Tabela::nova(1, 1);
    let mut c = celula("ABC", false);
    c.estilo.negrito = true;
    tabela.definir(0, 0, c).unwrap();
    let fonte = |texto: &str, estilo: &EstiloCelula| {
        MedidaTexto {
            largura: texto.chars().count() as f32 * if estilo.negrito { 8.0 } else { 4.0 },
            altura_linha: 12.0,
        }
    };
    let layout = LayoutTabela::default();
    assert_eq!(layout.largura_auto(&tabela, 0, &fonte), Ok(24.0));
    assert_eq!(layout.medir_celula(&tabela, 0, 0, &fonte).unwrap().altura, 12.0);
}

#[test]
fn medicao_nan_ou_invalida_nao_altera_tabela() {
    let mut tabela = Tabela::nova(2, 2);
    tabela.definir(0, 0, celula("conteudo", true)).unwrap();
    let antes = tabela.clone();
    let invalida = |_t: &str, _s: &EstiloCelula| MedidaTexto {
        largura: f32::NAN,
        altura_linha: 10.0,
    };
    assert_eq!(
        LayoutTabela::default().auto_ajustar_coluna(&mut tabela, 0, &invalida),
        Err(ErroLayout::MedicaoInvalida)
    );
    assert_eq!(tabela, antes);
    let vazia = |_t: &str, _s: &EstiloCelula| MedidaTexto {
        largura: 0.0,
        altura_linha: 0.0,
    };
    assert_eq!(
        LayoutTabela::default().auto_ajustar_linha(&mut tabela, 0, &vazia),
        Err(ErroLayout::MedicaoInvalida)
    );
    assert_eq!(tabela, antes);
}

#[test]
fn rejeita_configuracao_indices_e_janelas_excessivas() {
    let mut cfg = ConfigLayout::default();
    cfg.largura_maxima = f32::NAN;
    assert_eq!(LayoutTabela::novo(cfg), Err(ErroLayout::ConfiguracaoInvalida));
    let tabela = Tabela::nova(500_000, 500_000);
    let mut layout = LayoutTabela::default();
    assert_eq!(layout.ocultar_linha(&tabela, 500_000, true), Err(ErroLayout::CoordenadaInvalida));
    assert_eq!(layout.ocultar_coluna(&tabela, 500_000, true), Err(ErroLayout::CoordenadaInvalida));
    assert_eq!(layout.linhas_visiveis(&tabela, 10, 5), Err(ErroLayout::CoordenadaInvalida));
    assert_eq!(layout.linhas_visiveis(&tabela, 0, 100_001), Err(ErroLayout::JanelaExcessiva));
    assert_eq!(layout.colunas_visiveis(&tabela, 1, 500_001), Err(ErroLayout::CoordenadaInvalida));
    assert_eq!(layout.largura_auto(&tabela, 500_000, &medir()), Err(ErroLayout::CoordenadaInvalida));
}

#[test]
fn linhas_vazias_e_quebras_explicitamente_preservadas() {
    let mut tabela = Tabela::nova(1, 1);
    tabela.definir(0, 0, celula("a\n\nb\n", false)).unwrap();
    let r = LayoutTabela::default().medir_celula(&tabela, 0, 0, &medir()).unwrap();
    assert_eq!(r.linhas, vec!["a", "", "b", ""]);
    assert_eq!(r.altura, 40.0);
}

#[test]
fn altura_auto_desconsidera_linhas_e_colunas_ocultas() {
    let mut tabela = Tabela::nova(2, 2);
    tabela.definir(0, 0, celula("x", false)).unwrap();
    tabela.definir(0, 1, celula("a\nb\nc", false)).unwrap();
    let mut layout = LayoutTabela::default();
    assert_eq!(layout.altura_auto(&tabela, 0, &medir()), Ok(30.0));
    layout.ocultar_coluna(&tabela, 1, true).unwrap();
    assert_eq!(layout.altura_auto(&tabela, 0, &medir()), Ok(16.0));
    layout.ocultar_linha(&tabela, 0, true).unwrap();
    assert_eq!(layout.largura_auto(&tabela, 0, &medir()), Ok(16.0));
}


#[test]
fn paginacao_respeita_alturas_e_linhas_ocultas_sem_fundir_linhas() {
    let mut tabela = Tabela::nova(10, 1);
    tabela.definir_altura(2, 40.0).unwrap();
    let mut layout = LayoutTabela::default();
    layout.ocultar_linha(&tabela, 1, true).unwrap();
    let paginas = layout.paginar_linhas(&tabela, 0, 4, 50.0).unwrap();
    assert_eq!(paginas.len(), 3);
    assert_eq!((paginas[0].inicio, paginas[0].fim, paginas[0].altura), (0, 1, 24.0));
    assert_eq!((paginas[1].inicio, paginas[1].fim, paginas[1].altura), (2, 3, 40.0));
    assert_eq!((paginas[2].inicio, paginas[2].fim, paginas[2].altura), (3, 4, 24.0));
    assert_eq!(
        layout.paginar_linhas(&tabela, 0, 3, 0.0),
        Err(ErroLayout::ConfiguracaoInvalida)
    );
}

#[test]
fn paginacao_coloca_linha_mais_alta_que_pagina_sozinha() {
    let mut tabela = Tabela::nova(3, 1);
    tabela.definir_altura(1, 120.0).unwrap();
    let paginas = LayoutTabela::default()
        .paginar_linhas(&tabela, 0, 3, 60.0)
        .unwrap();
    assert_eq!(paginas.len(), 3);
    assert_eq!((paginas[1].inicio, paginas[1].fim, paginas[1].altura), (1, 2, 120.0));
}


#[test]
fn estilo_em_celula_vazia_contribui_para_autofit_sem_dense_grid() {
    let mut tabela = Tabela::nova(1_000_000, 1_000_000);
    let mut estilo = EstiloLayout::default();
    estilo.padding.esquerda = 30.0;
    estilo.padding.direita = 20.0;
    estilo.padding.cima = 20.0;
    estilo.padding.baixo = 15.0;
    tabela.definir_layout(900_000, 900_000, estilo.clone()).unwrap();
    assert_eq!(tabela.iter().count(), 0);
    assert_eq!(tabela.iter_layout().count(), 1);
    let visao = LayoutTabela::default();
    assert_eq!(visao.largura_auto(&tabela, 900_000, &medir()), Ok(50.0));
    assert_eq!(visao.altura_auto(&tabela, 900_000, &medir()), Ok(45.0));
    assert_eq!(tabela.layout(900_000, 900_000), Some(&estilo));
}

#[test]
fn quebras_crlf_e_cr_nao_modificam_texto_de_origem() {
    let mut tabela = Tabela::nova(1, 1);
    tabela.definir(0, 0, celula("um\r\ndois\rtres", true)).unwrap();
    let antes = tabela.clone();
    let medido = LayoutTabela::default()
        .medir_celula(&tabela, 0, 0, &medir())
        .unwrap();
    assert_eq!(medido.linhas, vec!["um", "dois", "tres"]);
    assert_eq!(medido.altura, 30.0);
    assert_eq!(tabela, antes);
}
