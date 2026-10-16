//! Regressões de TBL-006: regiões esparsas, subdivisão, normalização e undo.
use feather_tables::{
    Celula, DivisaoMesclagem, ErroTabela, EstiloLayout, Regiao, Tabela, Valor,
};

fn regiao(linha: usize, coluna: usize, linhas: usize, colunas: usize) -> Regiao {
    Regiao { linha, coluna, linhas, colunas }
}

fn texto(valor: &str) -> Celula {
    Celula {
        valor: Valor::Texto(valor.to_owned()),
        ..Celula::default()
    }
}

#[test]
fn mesclar_undo_refazer_preserva_tabela_esparsa_e_ordena_regioes() {
    let mut tabela = Tabela::nova(1_000_000, 1_000_000);
    tabela.definir(500_000, 500_000, texto("longe")).unwrap();
    tabela.definir_largura(500_000, 52.0).unwrap();
    tabela.definir_altura(500_000, 31.0).unwrap();
    let mut layout = EstiloLayout::default();
    layout.padding.esquerda = 5.0;
    tabela.definir_layout(500_000, 500_000, layout).unwrap();
    let a = regiao(500_000, 500_000, 4, 4);
    let b = regiao(2, 2, 2, 2);
    tabela.mesclar(a).unwrap();
    let edicao = tabela.mesclar_com_undo(b).unwrap();
    assert_eq!(tabela.mesclagens(), &[b, a]);
    assert_eq!(edicao.antes(), &[a]);
    assert_eq!(edicao.depois(), &[b, a]);

    edicao.desfazer(&mut tabela).unwrap();
    assert_eq!(tabela.mesclagens(), &[a]);
    edicao.refazer(&mut tabela).unwrap();
    assert_eq!(tabela.mesclagens(), &[b, a]);
    assert_eq!(tabela.iter().count(), 1);
    assert_eq!(tabela.obter(500_000, 500_000).unwrap().valor, Valor::Texto("longe".into()));
    assert_eq!(tabela.largura(500_000), Some(52.0));
    assert_eq!(tabela.altura(500_000), Some(31.0));
    assert_eq!(tabela.layout(500_000, 500_000).unwrap().padding.esquerda, 5.0);
}

#[test]
fn desmesclar_pela_celula_interna_pode_ser_desfeito() {
    let mut tabela = Tabela::nova(8, 8);
    let reg = regiao(1, 2, 3, 4);
    tabela.definir(1, 2, texto("âncora")).unwrap();
    tabela.mesclar(reg).unwrap();
    let edicao = tabela.desmesclar_com_undo(3, 5).unwrap();
    assert!(tabela.mesclagens().is_empty());
    tabela.definir(3, 5, Celula::default()).unwrap();
    edicao.desfazer(&mut tabela).unwrap();
    assert_eq!(tabela.mesclagens(), &[reg]);
    assert_eq!(
        tabela.definir(3, 5, texto("inválido")),
        Err(ErroTabela::Sobreposicao)
    );
    assert_eq!(tabela.obter(1, 2).unwrap().valor, Valor::Texto("âncora".into()));
    edicao.refazer(&mut tabela).unwrap();
    assert_eq!(tabela.mesclagens().len(), 0);
}

#[test]
fn cortes_horizontais_verticais_e_remescla_sao_reversiveis() {
    let mut tabela = Tabela::nova(12, 12);
    let original = regiao(1, 1, 4, 4);
    tabela.definir(1, 1, texto("único")).unwrap();
    tabela.mesclar(original).unwrap();

    let horizontal = tabela
        .dividir_mesclagem(3, 3, DivisaoMesclagem::Horizontal, 3)
        .unwrap();
    assert_eq!(tabela.mesclagens(), &[regiao(1, 1, 2, 4), regiao(3, 1, 2, 4)]);
    let vertical = tabela
        .dividir_mesclagem(4, 3, DivisaoMesclagem::Vertical, 3)
        .unwrap();
    assert_eq!(
        tabela.mesclagens(),
        &[regiao(1, 1, 2, 4), regiao(3, 1, 2, 2), regiao(3, 3, 2, 2)]
    );
    assert_eq!(
        horizontal.desfazer(&mut tabela),
        Err(ErroTabela::MesclagemInvalida)
    );
    vertical.desfazer(&mut tabela).unwrap();
    horizontal.desfazer(&mut tabela).unwrap();
    assert_eq!(tabela.mesclagens(), &[original]);
    horizontal.refazer(&mut tabela).unwrap();
    vertical.refazer(&mut tabela).unwrap();

    let reagrupamento = tabela.remesclar_com_undo(original).unwrap();
    assert_eq!(tabela.mesclagens(), &[original]);
    reagrupamento.desfazer(&mut tabela).unwrap();
    assert_eq!(tabela.mesclagens().len(), 3);
    reagrupamento.refazer(&mut tabela).unwrap();
    assert_eq!(tabela.mesclagens(), &[original]);
    assert_eq!(tabela.obter(1, 1).unwrap().valor, Valor::Texto("único".into()));
}

#[test]
fn subdivisao_que_gera_celulas_unitarias_elimina_mesclagem() {
    let mut tabela = Tabela::nova(2, 2);
    let original = regiao(0, 0, 1, 2);
    tabela.mesclar(original).unwrap();
    let edicao = tabela
        .dividir_mesclagem(0, 1, DivisaoMesclagem::Vertical, 1)
        .unwrap();
    assert!(tabela.mesclagens().is_empty());
    edicao.desfazer(&mut tabela).unwrap();
    assert_eq!(tabela.mesclagens(), &[original]);
}

#[test]
fn cortes_e_remesclagens_invalidos_sao_atomicos() {
    let mut tabela = Tabela::nova(5, 5);
    tabela.mesclar(regiao(1, 1, 2, 2)).unwrap();
    let original = tabela.clone();
    assert_eq!(
        tabela.dividir_mesclagem(1, 1, DivisaoMesclagem::Horizontal, 1),
        Err(ErroTabela::MesclagemInvalida)
    );
    assert_eq!(
        tabela.dividir_mesclagem(1, 1, DivisaoMesclagem::Vertical, 3),
        Err(ErroTabela::MesclagemInvalida)
    );
    assert_eq!(
        tabela.remesclar(regiao(2, 2, 2, 2)),
        Err(ErroTabela::Sobreposicao)
    );
    assert_eq!(
        tabela.desmesclar(4, 4),
        Err(ErroTabela::MesclagemInvalida)
    );
    assert_eq!(
        tabela.desmesclar(10, 10),
        Err(ErroTabela::ForaDosLimites)
    );
    assert_eq!(
        tabela.remesclar(regiao(0, 0, usize::MAX, 1)),
        Err(ErroTabela::MesclagemInvalida)
    );
    assert_eq!(tabela, original);
}

#[test]
fn redo_bloqueia_ocultacao_de_dados_que_surgiram_depois_do_undo() {
    let mut tabela = Tabela::nova(3, 3);
    let edicao = tabela.mesclar_com_undo(regiao(0, 0, 2, 2)).unwrap();
    edicao.desfazer(&mut tabela).unwrap();
    tabela.definir(1, 1, texto("não pode sumir")).unwrap();
    let antes = tabela.clone();
    assert_eq!(edicao.refazer(&mut tabela), Err(ErroTabela::DadosNaMesclagem));
    assert_eq!(tabela, antes);
}

#[test]
fn normalizacao_e_reagrupamento_respeitam_layouts_em_outras_ancoras() {
    let mut tabela = Tabela::nova(8, 8);
    tabela.mesclar(regiao(3, 3, 2, 2)).unwrap();
    tabela.mesclar(regiao(0, 0, 2, 2)).unwrap();
    tabela.normalizar_mesclagens().unwrap();
    assert_eq!(
        tabela.mesclagens(),
        &[regiao(0, 0, 2, 2), regiao(3, 3, 2, 2)]
    );
    let mut estilo = EstiloLayout::default();
    estilo.padding.cima = 7.0;
    tabela.definir_layout(3, 3, estilo).unwrap();
    let antes = tabela.clone();
    assert_eq!(
        tabela.remesclar(regiao(2, 2, 4, 4)),
        Err(ErroTabela::DadosNaMesclagem)
    );
    assert_eq!(tabela, antes);
}
