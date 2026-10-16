//! Exemplo sem GUI: mesmo runtime JS para hosts de produtos diferentes.
//! cargo run -p feather-macros
//!     --example mac001_motor_comum -- /tmp/featheroffice-macros-demo.json
use feather_macros::{executar, Catalogo, HospedeiroMacros};
use serde_json::{json, Value as Json};
use std::{cell::RefCell, path::PathBuf, rc::Rc};

struct Produto {
    nome: &'static str,
    titulo: RefCell<String>,
}

impl HospedeiroMacros for Produto {
    fn chamar(&self, operacao: &str, dados: &Json) -> Result<Json, String> {
        match operacao {
            "nome" => Ok(json!(self.nome)),
            "titulo" => Ok(json!(self.titulo.borrow().as_str())),
            "alterarTitulo" => {
                let titulo = dados.get(0).and_then(Json::as_str)
                    .ok_or("Título precisa ser texto")?;
                *self.titulo.borrow_mut() = titulo.to_owned();
                Ok(Json::Null)
            }
            _ => Err(format!("{operacao}: operação não autorizada neste produto")),
        }
    }
}

fn executar_produto(nome: &'static str) {
    let host = Rc::new(Produto {
        nome, titulo: RefCell::new("Documento".to_string()),
    });
    // Somente a API do produto é permitida; sem DOM/rede/arquivos.
    let fonte = r#"
        log(__ft('nome', []));
        __ft('alterarTitulo', ['Título via macro']);
        log(__ft('titulo', []));
    "#;
    let resultado = executar(host.clone(), "", fonte);
    assert!(resultado.sucesso(), "{:?}", resultado.erro);
    assert_eq!(resultado.saida, vec![nome.to_string(), "Título via macro".to_string()]);
    assert_eq!(*host.titulo.borrow(), "Título via macro");
    println!("{nome}: {:?}", resultado.saida);
}

fn main() {
    for produto in ["FeatherSheet", "FeatherSlide", "FeatherDocs"] {
        executar_produto(produto);
    }
    let original = r#"[{"name":"Saudacao","code":"log('olá')","auto":true}]"#;
    let catalogo = Catalogo::importar_json(original).unwrap();
    let caminho = std::env::args().nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("featheroffice-macros-demo.json"));
    catalogo.salvar_em(&caminho).unwrap();
    let lido = Catalogo::carregar_de(&caminho).unwrap();
    assert_eq!(lido, catalogo);
    println!("Catálogo compartilhado: {} ({})", caminho.display(), lido.macros.len());
}
