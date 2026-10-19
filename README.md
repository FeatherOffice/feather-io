# feather-io

Motores comuns da suíte **FeatherWorks** para **arquivos, tabelas, imagens e
macros**, em Rust puro. São bibliotecas, não aplicativos: o
[FeatherSheet](https://github.com/feather-works/feathersheet), o FeatherDocs,
o FeatherSlide e o FeatherPDF usam o mesmo código para ler e gravar formatos,
guardar tabelas, tratar imagens e rodar macros.

## Crates

| Crate | Para que serve |
|---|---|
| [`feather-io`](crates/feather-io) | registro de formatos e conversão entre eles: detecção pelo conteúdo, `.csv`, gravação de `.xlsx`, `.docx` e `.pptx` (tabelas), exportação de gráficos e imagens, limites de segurança para arquivos hostis |
| [`feather-tables`](crates/feather-tables) | modelo esparso de tabelas e estilos reutilizáveis: células, seleção, mesclas e layout |
| [`feather-image`](crates/feather-image) | operações em pixels RGBA: efeitos (cinza, desfoque, nitidez), redimensionar, Base64 |
| [`feather-macros`](crates/feather-macros) | motor de macros JavaScript (interpretador [Boa](https://github.com/boa-dev/boa)), rodando só na máquina |

Os crates dependem de [`feather-charts`](https://github.com/feather-works/feather-charts)
(gráficos) e [`feather-draw`](https://github.com/feather-works/feather-draw)
(desenho vetorial, SVG e PDF).

## Usar

```toml
[dependencies]
feather-io = { git = "https://github.com/feather-works/feather-io" }
feather-tables = { git = "https://github.com/feather-works/feather-io" }
```

Enquanto o `feather-charts` é usado por caminho, clone os dois lado a lado:

```text
GitHub/
├── feather-io/
└── feather-charts/
```

## Testes

```sh
cargo test --workspace
```

Os arquivos de referência (corpus dourado) ficam em [`fixtures/`](fixtures).

## Privacidade

Nenhum crate faz acesso à rede. Leitura e gravação acontecem só nos bytes que
o aplicativo entrega. As macros rodam num interpretador local; o `feather-macros`
só lê e grava a biblioteca de macros no caminho que o aplicativo indicar.

## Licença

[GPL-3.0](LICENSE).
