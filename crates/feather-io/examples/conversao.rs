//! Conversão CSV com vírgulas, aspas e quebras de linha: fixture de round-trip.
use feather_io::{Documento, Registro};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let entrada = include_bytes!("../../../fixtures/conversao.csv");
    let esperado = include_bytes!("../../../fixtures/conversao-esperado.csv");
    let registro = Registro::padrao();
    let Documento::Tabela(tabela) = registro
        .ler(".CSV", entrada)
        .map_err(|e| format!("{e:?}"))?
    else {
        panic!("codec CSV deve carregar tabela");
    };
    assert_eq!(tabela.dimensoes(), (3, 2));
    assert_eq!(
        tabela.obter(1, 1).unwrap().valor.texto(),
        "Linha 1\nLinha 2"
    );
    assert_eq!(
        tabela.obter(2, 0).unwrap().valor.texto(),
        "Caixa \"Especial\""
    );
    let saida = registro
        .converter("csv", "csv", entrada)
        .map_err(|e| format!("{e:?}"))?;
    assert_eq!(&saida, esperado, "serialização CSV determinística");
    assert_eq!(
        registro.ler("csv", &saida).map_err(|e| format!("{e:?}"))?,
        Documento::Tabela(tabela),
        "round-trip sem perda de valores"
    );
    println!("OK conversão: CSV UTF-8, aspas, vírgulas, multiline");
    Ok(())
}
