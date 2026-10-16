//! Fixture CSV -> Tabela estilizada e mesclada -> CSV.
use feather_io::{Documento, Registro};
use feather_tables::{Regiao, Valor};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let registro = Registro::padrao();
    let bytes = include_bytes!("../../../fixtures/tabela.csv");
    let Documento::Tabela(mut tabela) = registro.ler("csv", bytes).map_err(|e| format!("{e:?}"))?
    else {
        panic!("esperada tabela");
    };
    assert_eq!(tabela.dimensoes(), (3, 3));
    assert_eq!(
        tabela.obter(1, 0).unwrap().valor,
        Valor::Texto("Slide".into())
    );
    let mut celula = tabela.obter(1, 0).unwrap().clone();
    celula.estilo.negrito = true;
    tabela.definir(1, 0, celula).map_err(|e| format!("{e:?}"))?;
    tabela
        .mesclar(Regiao {
            linha: 1,
            coluna: 2,
            linhas: 2,
            colunas: 1,
        })
        .map_err(|e| format!("{e:?}"))?;
    assert!(tabela.obter(1, 0).unwrap().estilo.negrito);
    assert_eq!(tabela.mesclagens().len(), 1);
    let csv = registro
        .gravar("csv", &Documento::Tabela(tabela))
        .map_err(|e| format!("{e:?}"))?;
    assert!(std::str::from_utf8(&csv)?.contains("Slide,10,"));
    println!("OK tabela: CSV, estilo e mesclagem");
    Ok(())
}
