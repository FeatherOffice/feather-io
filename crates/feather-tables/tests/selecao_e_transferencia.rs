//! TBL-007: seleção, navegação e área de transferência sem dependência de GUI.
use feather_tables::{
    AreaCopiada, Celula, ErroSelecao, EstiloCelula, EstiloLayout, ModoColagem,
    Navegacao, PosicaoCelula as P, Regiao, SelecaoTabela, Tabela, Valor,
};

fn bloco(linha: usize, coluna: usize, linhas: usize, colunas: usize) -> Regiao {
    Regiao { linha, coluna, linhas, colunas }
}
fn texto(texto: &str) -> Celula {
    Celula { valor: Valor::Texto(texto.to_owned()), ..Default::default() }
}

#[test]
fn selecao_retangular_reversa_e_shift_sem_mutar_tabela() {
    let tabela = Tabela::nova(8, 9);
    let mut selecao = SelecaoTabela::nova(&tabela, P::nova(5, 6)).unwrap();
    selecao.selecionar_ate(&tabela, P::nova(3, 2)).unwrap();
    assert_eq!(selecao.regiao(), bloco(3, 2, 3, 5));
    selecao.mover(&tabela, Navegacao::Esquerda, true).unwrap();
    assert_eq!(selecao.ancora, P::nova(5, 6));
    assert_eq!(selecao.ativo, P::nova(3, 1));
    selecao.mover(&tabela, Navegacao::Direita, false).unwrap();
    assert_eq!(selecao.regiao(), bloco(3, 2, 1, 1));
}

#[test]
fn navegacao_tab_volta_linha_e_respeita_bordas() {
    let tabela = Tabela::nova(2, 3);
    let mut s = SelecaoTabela::nova(&tabela, P::nova(0, 2)).unwrap();
    s.mover(&tabela, Navegacao::Tab, false).unwrap();
    assert_eq!(s.ativo, P::nova(1, 0));
    s.mover(&tabela, Navegacao::ShiftTab, false).unwrap();
    assert_eq!(s.ativo, P::nova(0, 2));
    s.mover(&tabela, Navegacao::InicioLinha, false).unwrap();
    assert_eq!(s.ativo, P::nova(0, 0));
    s.mover(&tabela, Navegacao::ShiftTab, false).unwrap();
    assert_eq!(s.ativo, P::nova(0, 0));
    s.mover(&tabela, Navegacao::FimTabela, false).unwrap();
    assert_eq!(s.ativo, P::nova(1, 2));
    s.mover(&tabela, Navegacao::Direita, false).unwrap();
    assert_eq!(s.ativo, P::nova(1, 2));
    s.mover(&tabela, Navegacao::Baixo, false).unwrap();
    assert_eq!(s.ativo, P::nova(1, 2));
    s.mover(&tabela, Navegacao::InicioTabela, false).unwrap();
    assert_eq!(s.ativo, P::nova(0, 0));
    s.mover(&tabela, Navegacao::FimLinha, false).unwrap();
    assert_eq!(s.ativo, P::nova(0, 2));
}

#[test]
fn cursor_salta_mescla_e_clique_interno_usa_ancora() {
    let mut tabela = Tabela::nova(8, 8);
    tabela.mesclar(bloco(2, 2, 2, 3)).unwrap();
    let mut s = SelecaoTabela::nova(&tabela, P::nova(3, 3)).unwrap();
    assert_eq!(s.ativo, P::nova(2, 2));
    s.mover(&tabela, Navegacao::Direita, false).unwrap();
    assert_eq!(s.ativo, P::nova(2, 5));
    s.mover(&tabela, Navegacao::Esquerda, false).unwrap();
    assert_eq!(s.ativo, P::nova(2, 2));
    s.mover(&tabela, Navegacao::Baixo, false).unwrap();
    assert_eq!(s.ativo, P::nova(4, 2));
    s.mover(&tabela, Navegacao::Cima, false).unwrap();
    assert_eq!(s.ativo, P::nova(2, 2));
}

#[test]
fn coordenadas_invalidas_nao_mutam_selecao() {
    let tabela = Tabela::nova(2, 2);
    assert_eq!(
        SelecaoTabela::nova(&tabela, P::nova(2, 0)),
        Err(ErroSelecao::ForaDosLimites)
    );
    let mut selecao = SelecaoTabela::nova(&tabela, P::nova(0, 0)).unwrap();
    let anterior = selecao;
    assert_eq!(
        selecao.selecionar_ate(&tabela, P::nova(10, 0)),
        Err(ErroSelecao::ForaDosLimites)
    );
    assert_eq!(selecao, anterior);
}

#[test]
fn copia_rica_preserva_valores_tipados_bordas_e_layouts() {
    let mut origem = Tabela::nova(4, 4);
    let mut a = Celula {
        valor: Valor::Numero(13.5),
        estilo: EstiloCelula { negrito: true, fundo: Some([1, 2, 3, 255]), ..Default::default() },
    };
    a.estilo.bordas[0].espessura = 3;
    origem.definir(1, 1, a.clone()).unwrap();
    origem.definir(2, 2, Celula { valor: Valor::Logico(true), ..Default::default() }).unwrap();
    let mut layout = EstiloLayout::default();
    layout.padding.esquerda = 7.0;
    layout.entre_linhas = 1.25;
    origem.definir_layout(1, 1, layout.clone()).unwrap();
    let mut so_layout = EstiloLayout::default();
    so_layout.padding.direita = 5.0;
    origem.definir_layout(2, 1, so_layout.clone()).unwrap();

    let copia = AreaCopiada::copiar(&origem, bloco(1, 1, 2, 2)).unwrap();
    assert_eq!(copia.dimensoes(), (2, 2));
    assert_eq!(copia.entradas_armazenadas(), 3);
    let mut destino = Tabela::nova(6, 6);
    assert_eq!(copia.colar(&mut destino, P::nova(3, 3), ModoColagem::Tudo), Ok(bloco(3, 3, 2, 2)));
    assert_eq!(destino.obter(3, 3), Some(&a));
    assert_eq!(destino.obter(4, 4).unwrap().valor, Valor::Logico(true));
    assert!(destino.obter(4, 3).is_none());
    assert_eq!(destino.layout(3, 3), Some(&layout));
    assert_eq!(destino.layout(4, 3), Some(&so_layout));
    assert!(destino.obter(3, 4).is_none());
}

#[test]
fn colagem_valores_preserva_estilos_destino_e_limpa_vazios() {
    let mut tabela = Tabela::nova(4, 4);
    let mut celula = texto("destino");
    celula.estilo.fonte = Some("Inter".into());
    tabela.definir(2, 2, celula.clone()).unwrap();
    tabela.definir(2, 3, texto("deve limpar")).unwrap();
    let mut layout = EstiloLayout::default();
    layout.padding.cima = 4.0;
    tabela.definir_layout(2, 2, layout.clone()).unwrap();

    let mut fonte = Tabela::nova(1, 2);
    fonte.definir(0, 0, texto("novo")).unwrap();
    let copia = AreaCopiada::copiar(&fonte, bloco(0, 0, 1, 2)).unwrap();
    copia.colar(&mut tabela, P::nova(2, 2), ModoColagem::SomenteValores).unwrap();
    assert_eq!(tabela.obter(2, 2).unwrap().valor, Valor::Texto("novo".into()));
    assert_eq!(tabela.obter(2, 2).unwrap().estilo, celula.estilo);
    assert_eq!(tabela.layout(2, 2), Some(&layout));
    assert!(tabela.obter(2, 3).is_none());
}

#[test]
fn colagem_estilos_preserva_valores_e_limpa_formatacao_faltante() {
    let mut fonte = Tabela::nova(1, 2);
    let mut celula = texto("texto origem");
    celula.estilo.italico = true;
    fonte.definir(0, 0, celula.clone()).unwrap();
    let mut layout = EstiloLayout::default();
    layout.padding.esquerda = 2.0;
    fonte.definir_layout(0, 0, layout.clone()).unwrap();

    let mut destino = Tabela::nova(2, 2);
    destino.definir(0, 0, texto("valor 1")).unwrap();
    let mut segunda = texto("valor 2");
    segunda.estilo.negrito = true;
    destino.definir(0, 1, segunda).unwrap();
    destino.definir_layout(0, 1, layout.clone()).unwrap();

    let copia = AreaCopiada::copiar(&fonte, bloco(0, 0, 1, 2)).unwrap();
    copia.colar(&mut destino, P::nova(0, 0), ModoColagem::SomenteEstilos).unwrap();
    assert_eq!(destino.obter(0, 0).unwrap().valor, Valor::Texto("valor 1".into()));
    assert_eq!(destino.obter(0, 1).unwrap().valor, Valor::Texto("valor 2".into()));
    assert!(destino.obter(0, 0).unwrap().estilo.italico);
    assert_eq!(destino.layout(0, 0), Some(&layout));
    assert_eq!(destino.obter(0, 1).unwrap().estilo, EstiloCelula::default());
    assert!(destino.layout(0, 1).is_none());
}

#[test]
fn tudo_apaga_valores_e_layouts_em_posicoes_vazias() {
    let origem = Tabela::nova(1, 1);
    let copia = AreaCopiada::copiar(&origem, bloco(0, 0, 1, 1)).unwrap();
    let mut destino = Tabela::nova(1, 1);
    destino.definir(0, 0, texto("antigo")).unwrap();
    let mut layout = EstiloLayout::default();
    layout.padding.esquerda = 3.0;
    destino.definir_layout(0, 0, layout).unwrap();
    copia.colar(&mut destino, P::nova(0, 0), ModoColagem::Tudo).unwrap();
    assert!(destino.obter(0, 0).is_none());
    assert!(destino.layout(0, 0).is_none());
}

#[test]
fn colagem_fora_limites_e_mesclas_sao_rejeitadas_sem_alteracoes() {
    let origem = Tabela::nova(2, 2);
    let copia = AreaCopiada::copiar(&origem, bloco(0, 0, 2, 2)).unwrap();
    let mut destino = Tabela::nova(4, 4);
    destino.definir(0, 0, texto("manter")).unwrap();
    destino.mesclar(bloco(2, 2, 2, 2)).unwrap();
    let antes = destino.clone();
    assert_eq!(
        copia.colar(&mut destino, P::nova(3, 0), ModoColagem::Tudo),
        Err(ErroSelecao::ForaDosLimites)
    );
    assert_eq!(
        copia.colar(&mut destino, P::nova(1, 1), ModoColagem::Tudo),
        Err(ErroSelecao::MesclagemInterrompida)
    );
    assert_eq!(destino, antes);
}

#[test]
fn copia_parcial_ou_total_de_mescla_e_recusada_sem_perder_dados() {
    let mut tabela = Tabela::nova(6, 6);
    tabela.definir(1, 1, texto("ancora")).unwrap();
    tabela.mesclar(bloco(1, 1, 2, 2)).unwrap();
    for area in [bloco(1, 1, 1, 1), bloco(1, 1, 2, 2), bloco(2, 2, 1, 1)] {
        assert_eq!(
            AreaCopiada::copiar(&tabela, area),
            Err(ErroSelecao::MesclagemInterrompida)
        );
    }
    assert_eq!(tabela.obter(1, 1).unwrap().valor, Valor::Texto("ancora".into()));
    assert!(AreaCopiada::copiar(&tabela, bloco(4, 4, 1, 1)).is_ok());
}

#[test]
fn tsv_escapa_tabs_unicode_aspas_e_quebras_multilinha() {
    let entrada = "Produto\t\"descrição\tcom tab\"\r\n\"ação\nrápida\"\t\"a\"\"b\"\r\n";
    let copiado = AreaCopiada::de_tsv(entrada).unwrap();
    assert_eq!(copiado.dimensoes(), (2, 2));
    assert_eq!(
        copiado.para_tsv().unwrap(),
        "Produto\t\"descrição\tcom tab\"\n\"ação\nrápida\"\t\"a\"\"b\""
    );
    let mut tabela = Tabela::nova(2, 2);
    copiado.colar(&mut tabela, P::nova(0, 0), ModoColagem::Tudo).unwrap();
    assert_eq!(
        tabela.obter(0, 1).unwrap().valor,
        Valor::Texto("descrição\tcom tab".into())
    );
    assert_eq!(
        tabela.obter(1, 0).unwrap().valor,
        Valor::Texto("ação\nrápida".into())
    );
    assert_eq!(tabela.obter(1, 1).unwrap().valor, Valor::Texto("a\"b".into()));
}

#[test]
fn tsv_textual_preserva_campos_vazios_e_nao_executa_formulas() {
    let entrada = "abc\t\t=1+2\r\n\txyz\t";
    let copia = AreaCopiada::de_tsv(entrada).unwrap();
    assert_eq!(copia.dimensoes(), (2, 3));
    assert_eq!(copia.entradas_armazenadas(), 3);
    let mut tabela = Tabela::nova(2, 3);
    copia.colar(&mut tabela, P::nova(0, 0), ModoColagem::Tudo).unwrap();
    assert!(tabela.obter(0, 1).is_none());
    assert!(tabela.obter(1, 0).is_none());
    assert!(tabela.obter(1, 2).is_none());
    assert_eq!(tabela.obter(0, 2).unwrap().valor, Valor::Texto("=1+2".into()));
    assert_eq!(copia.para_tsv().unwrap(), "abc\t\t=1+2\n\txyz\t");
}

#[test]
fn tsv_malformado_limites_e_posicao_zero_rejeitados() {
    for texto in ["\"sem fechar", "sem\"aspas", "\"fechado\"depois", "\"abc\"\t\"sem fechar"] {
        assert_eq!(AreaCopiada::de_tsv(texto), Err(ErroSelecao::TextoInvalido));
    }
    assert_eq!(
        AreaCopiada::de_tsv(&"x".repeat(8 * 1024 * 1024 + 1)),
        Err(ErroSelecao::AreaExcessiva)
    );
    let tabela = Tabela::nova(1_000_000, 1_000_000);
    assert_eq!(
        AreaCopiada::copiar(&tabela, bloco(0, 0, 400, 400)),
        Err(ErroSelecao::AreaExcessiva)
    );
    assert_eq!(
        AreaCopiada::copiar(&tabela, bloco(usize::MAX, 0, 1, 1)),
        Err(ErroSelecao::ForaDosLimites)
    );
    assert_eq!(
        SelecaoTabela::nova(&Tabela::nova(0, 0), P::nova(0, 0)),
        Err(ErroSelecao::ForaDosLimites)
    );
}

#[test]
fn grade_esparsa_milhoes_de_celulas_copia_so_armazenadas() {
    let mut tabela = Tabela::nova(2_000_000, 2_000_000);
    tabela.definir(1_000_001, 1_000_002, texto("localizado")).unwrap();
    let regiao = bloco(1_000_000, 1_000_000, 300, 300);
    let copia = AreaCopiada::copiar(&tabela, regiao).unwrap();
    assert_eq!(copia.entradas_armazenadas(), 1);
    assert_eq!(copia.dimensoes(), (300, 300));
    let mut destino = Tabela::nova(500, 500);
    copia.colar(&mut destino, P::nova(1, 1), ModoColagem::Tudo).unwrap();
    assert_eq!(
        destino.obter(2, 3).unwrap().valor,
        Valor::Texto("localizado".into())
    );
    assert_eq!(destino.iter().count(), 1);
}


#[derive(Default)]
struct ClipboardFalso {
    texto: String,
    falhar: bool,
}

impl feather_tables::AdaptadorTransferencia for ClipboardFalso {
    type Erro = &'static str;

    fn obter_texto(&mut self) -> Result<String, Self::Erro> {
        if self.falhar {
            Err("leitura negada")
        } else {
            Ok(self.texto.clone())
        }
    }

    fn definir_texto(&mut self, texto: &str) -> Result<(), Self::Erro> {
        if self.falhar {
            Err("escrita negada")
        } else {
            self.texto = texto.to_owned();
            Ok(())
        }
    }
}

#[test]
fn adaptador_textual_propagacao_erros_e_roundtrip_sem_gui() {
    use feather_tables::ErroAdaptador;
    let mut tabela = Tabela::nova(1, 1);
    tabela.definir(0, 0, texto("ação")).unwrap();
    let area = AreaCopiada::copiar(&tabela, bloco(0, 0, 1, 1)).unwrap();
    let mut clipboard = ClipboardFalso::default();
    area.copiar_para(&mut clipboard).unwrap();
    assert_eq!(clipboard.texto, "ação");
    let importada = AreaCopiada::receber_de(&mut clipboard).unwrap();
    assert_eq!(importada.para_tsv().unwrap(), "ação");
    clipboard.falhar = true;
    assert_eq!(
        area.copiar_para(&mut clipboard),
        Err(ErroAdaptador::Externo("escrita negada"))
    );
    assert_eq!(
        AreaCopiada::receber_de(&mut clipboard),
        Err(ErroAdaptador::Externo("leitura negada"))
    );
    clipboard.falhar = false;
    clipboard.texto = "\"aspas soltas".into();
    assert_eq!(
        AreaCopiada::receber_de(&mut clipboard),
        Err(ErroAdaptador::Selecao(ErroSelecao::TextoInvalido))
    );
}

#[test]
fn tsv_rejeita_milhares_de_campos_sem_alocar_grade_gigante() {
    let entrada = "\t".repeat(100_001);
    assert_eq!(AreaCopiada::de_tsv(&entrada), Err(ErroSelecao::AreaExcessiva));
}
