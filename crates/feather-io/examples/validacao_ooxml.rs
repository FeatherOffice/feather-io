//! Exemplo IO-010: grava tabelas reais XLSX, DOCX e PPTX.
use feather_io::{Documento, Registro};
use feather_tables::{Celula, Tabela, Valor};
use std::{env, fs, path::PathBuf};

fn main() {
    let destino = PathBuf::from(
        env::args().nth(1).unwrap_or_else(|| "/tmp/featheroffice-ooxml".into())
    );
    fs::create_dir_all(&destino).expect("criar diretório");
    let mut tabela = Tabela::nova(3, 3);
    for (linha, coluna, valor) in [
        (0, 0, Valor::Texto("Produto".into())),
        (0, 1, Valor::Texto("Quantidade".into())),
        (0, 2, Valor::Texto("Ativo".into())),
        (1, 0, Valor::Texto("Café & Pão".into())),
        (1, 1, Valor::Numero(12.5)),
        (1, 2, Valor::Logico(true)),
        (2, 0, Valor::Texto("Maçã <verde>".into())),
        (2, 1, Valor::Numero(-2.0)),
        (2, 2, Valor::Logico(false)),
    ] {
        tabela.definir(linha, coluna, Celula { valor, ..Default::default() })
            .expect("definir célula");
    }
    let registro = Registro::padrao();
    for ext in ["xlsx", "docx", "pptx"] {
        let bytes = registro.gravar(ext, &Documento::Tabela(tabela.clone()))
            .unwrap_or_else(|e| panic!("exportar {ext}: {e:?}"));
        let arquivo = destino.join(format!("tabela.{ext}"));
        fs::write(&arquivo, bytes).expect("gravar arquivo");
        eprintln!("gerado: {}", arquivo.display());
    }
}
