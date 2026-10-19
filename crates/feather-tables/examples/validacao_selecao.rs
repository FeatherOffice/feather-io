//! Demonstra seleção retangular, clipboard rico, TSV e colagem sem Iced.
//! cargo run -p feather-tables \
//!   --example validacao_selecao -j 2

use feather_tables::{
    AreaCopiada, Celula, ModoColagem, Navegacao, PosicaoCelula,
    Regiao, SelecaoTabela, Tabela, Valor,
};

fn main() {
    let mut tabela = Tabela::nova(6, 4);
    tabela.definir(0, 0, Celula {
        valor: Valor::Texto("Produto".into()),
        ..Default::default()
    }).expect("cabeçalho válido");
    tabela.definir(0, 1, Celula {
        valor: Valor::Texto("Preço".into()),
        ..Default::default()
    }).expect("cabeçalho válido");
    tabela.definir(1, 0, Celula {
        valor: Valor::Texto("FeatherWorks".into()),
        ..Default::default()
    }).expect("produto válido");
    tabela.definir(1, 1, Celula {
        valor: Valor::Numero(123.5),
        ..Default::default()
    }).expect("valor válido");

    let mut cursor = SelecaoTabela::nova(&tabela, PosicaoCelula::nova(0, 0))
        .expect("posição válida");
    cursor.mover(&tabela, Navegacao::Baixo, true).expect("navegação válida");
    cursor.mover(&tabela, Navegacao::Direita, true).expect("navegação válida");
    let selecionada = cursor.regiao();
    assert_eq!(selecionada, Regiao { linha: 0, coluna: 0, linhas: 2, colunas: 2 });
    let dados = AreaCopiada::copiar(&tabela, selecionada)
        .expect("cópia retangular válida");
    let tsv = dados.para_tsv().expect("serialização TSV");
    println!("Seleção: {selecionada:?}");
    println!("TSV:\n{tsv}");
    assert_eq!(tsv, "Produto\tPreço\nFeatherWorks\t123.5");

    dados.colar(&mut tabela, PosicaoCelula::nova(3, 1), ModoColagem::Tudo)
        .expect("colagem rica válida");
    assert_eq!(tabela.obter(4, 2).unwrap().valor, Valor::Numero(123.5));

    let texto_externo = AreaCopiada::de_tsv("\"Nova\tcoluna\"\t42")
        .expect("importação TSV válida");
    texto_externo.colar(&mut tabela, PosicaoCelula::nova(5, 0), ModoColagem::SomenteValores)
        .expect("colagem textual válida");
    assert_eq!(tabela.obter(5, 0).unwrap().valor, Valor::Texto("Nova\tcoluna".into()));
    assert_eq!(tabela.obter(5, 1).unwrap().valor, Valor::Texto("42".into()));
    println!("OK: navegação, cópia rica, TSV e colagem preservados.");
}
