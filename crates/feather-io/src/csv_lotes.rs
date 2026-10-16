//! Leitor permissivo CSV/TSV em lotes, compartilhado por FeatherOffice e FeatherSheet.
//!
//! Extraído do parser historicamente usado por `feathersheet-io::csv`.
//! O contrato preserva separadores, aspas e multilinhas do editor legado,
//! sem converter números: vírgulas decimais e outros valores de locale
//! permanecem `String` para interpretação pelo consumidor.
//!
//! Ao contrário do codec RFC estrito de `fluxo`, este leitor aceita texto
//! irregular como o antigo parser de planilhas. Os lotes evitam armazenar
//! simultaneamente a matriz completa de um arquivo grande.

/// Quantidade de linhas recomendada por lote para importação em planilhas.
pub const LINHAS_POR_LOTE: usize = 50_000;

/// Lê diretamente do disco, retendo apenas uma linha física e um lote.
/// `None` detecta o separador numa amostra de registros. Aspas multilinha mantêm
/// seu estado entre leituras; CR, LF e CRLF são preservados dentro de campos.
pub fn dividir_csv_leitor(
    leitor: impl std::io::BufRead,
    separador: Option<char>,
    lote: usize,
    entregar: impl FnMut(&mut Vec<Vec<String>>),
) -> std::io::Result<char> {
    match separador {
        Some(_) => dividir_csv_leitor_configurado(leitor, separador, lote, entregar),
        None => dividir_csv_leitor_preferindo(leitor, None, lote, entregar),
    }
}

/// Como `dividir_csv_leitor`, mas `preferido` (o separador do idioma) só
/// vale se dividir o cabeçalho; senão nada é dividido. `None` detecta.
/// Ver `escolher_separador`.
pub fn dividir_csv_leitor_preferindo(
    mut leitor: impl std::io::BufRead,
    preferido: Option<char>,
    lote: usize,
    entregar: impl FnMut(&mut Vec<Vec<String>>),
) -> std::io::Result<char> {
    {
        use std::io::Read;
        let mut amostra = Vec::new();
        leitor.by_ref().take(64 * 1024).read_to_end(&mut amostra)?;
        let texto = match std::str::from_utf8(&amostra) {
            Ok(s) => s,
            Err(e) if e.error_len().is_none() => {
                std::str::from_utf8(&amostra[..e.valid_up_to()]).unwrap()
            }
            Err(e) => return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        };
        let delimitador = escolher_separador(texto, preferido);
        let origem = std::io::Cursor::new(amostra).chain(leitor);
        dividir_csv_leitor_configurado(
            std::io::BufReader::new(origem),
            Some(delimitador),
            lote,
            entregar,
        )
    }
}

fn dividir_csv_leitor_configurado(
    mut leitor: impl std::io::BufRead,
    separador: Option<char>,
    lote: usize,
    mut entregar: impl FnMut(&mut Vec<Vec<String>>),
) -> std::io::Result<char> {
    let mut delim = separador;
    let mut primeira = true;
    let mut entre_aspas = false;
    let mut ignorar_lf = false;
    let mut campo = String::new();
    let mut linha = Vec::new();
    let mut linhas = Vec::new();
    let mut bytes = Vec::new();
    loop {
        bytes.clear();
        // Uma linha física por vez, também para arquivos que usam só CR.
        loop {
            let buffer = leitor.fill_buf()?;
            if buffer.is_empty() {
                break;
            }
            let fim = buffer.iter().position(|b| matches!(b, b'\r' | b'\n'));
            let n = fim.map_or(buffer.len(), |i| i + 1);
            bytes.extend_from_slice(&buffer[..n]);
            leitor.consume(n);
            if fim.is_some() {
                break;
            }
        }
        if bytes.is_empty() {
            break;
        }
        let texto = std::str::from_utf8(&bytes)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let texto = if primeira {
            texto.strip_prefix('\u{feff}').unwrap_or(texto)
        } else {
            texto
        };
        primeira = false;
        let d = *delim.get_or_insert_with(|| detectar_separador(texto));
        let mut chars = texto.chars().peekable();
        while let Some(c) = chars.next() {
            if std::mem::take(&mut ignorar_lf) && c == '\n' {
                continue;
            }
            if entre_aspas {
                if c == '"' {
                    if chars.peek() == Some(&'"') {
                        campo.push('"');
                        chars.next();
                    } else {
                        entre_aspas = false;
                    }
                } else {
                    campo.push(c);
                }
            } else if c == '"' && campo.is_empty() {
                entre_aspas = true;
            } else if c == d {
                linha.push(std::mem::take(&mut campo));
            } else if c == '\n' || c == '\r' {
                ignorar_lf = c == '\r';
                linha.push(std::mem::take(&mut campo));
                linhas.push(std::mem::take(&mut linha));
                if linhas.len() >= lote.max(1) {
                    entregar(&mut linhas);
                    linhas.clear();
                }
            } else {
                campo.push(c);
            }
        }
    }
    if !campo.is_empty() || !linha.is_empty() {
        linha.push(campo);
        linhas.push(linha);
    }
    if !linhas.is_empty() {
        entregar(&mut linhas);
    }
    Ok(delim.unwrap_or(','))
}

/// Devolvido quando o cabeçalho não se divide por nenhum candidato: o
/// arquivo abre numa coluna só, sem cortar as linhas de baixo. Nenhum
/// texto de verdade tem NUL, então o parser nunca encontra este separador.
pub const SEM_SEPARADOR: char = '\0';

/// Escolhe um único delimitador pela consistência de até 32 registros.
/// Aspas protegem os separadores e as quebras de linha; empates completos
/// favorecem vírgula. A amostra fica limitada a 64 KiB.
///
/// O cabeçalho manda: só concorre quem aparece no primeiro registro. Se o
/// cabeçalho não se divide mas as linhas de baixo sim (vírgula decimal num
/// arquivo de uma coluna, por exemplo), devolve `SEM_SEPARADOR`.
pub fn detectar_separador(conteudo: &str) -> char {
    escolher_separador(conteudo, None)
}

/// `preferido` (o separador do idioma, `;` em pt-BR, com a detecção
/// desligada) é o único aceito: vale se dividir o cabeçalho; se não divide,
/// devolve `SEM_SEPARADOR` e o arquivo abre numa coluna só, sem cortar as
/// linhas de baixo. `None` detecta entre os que dividem o cabeçalho.
pub fn escolher_separador(conteudo: &str, preferido: Option<char>) -> char {
    let mut fim = conteudo.len().min(64 * 1024);
    while !conteudo.is_char_boundary(fim) {
        fim -= 1;
    }
    let mut chars = conteudo[..fim]
        .trim_start_matches('\u{feff}')
        .chars()
        .peekable();
    let mut registros = Vec::new();
    let mut contagens = [0usize; 3];
    let mut citado = false;
    let mut inicio_campo = true;
    let mut tem_conteudo = false;
    while let Some(c) = chars.next() {
        if citado {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                } else {
                    citado = false;
                }
            }
            continue;
        }
        if c == '"' && inicio_campo {
            citado = true;
            tem_conteudo = true;
            continue;
        }
        if c == '\n' || c == '\r' {
            if c == '\r' && chars.peek() == Some(&'\n') {
                chars.next();
            }
            if tem_conteudo {
                registros.push(contagens);
            }
            if registros.len() == 32 {
                break;
            }
            contagens = [0; 3];
            inicio_campo = true;
            tem_conteudo = false;
            continue;
        }
        if let Some(i) = [',', ';', '\t'].iter().position(|d| *d == c) {
            contagens[i] += 1;
            inicio_campo = true;
        } else {
            inicio_campo = false;
        }
        tem_conteudo = true;
    }
    if registros.len() < 32 && tem_conteudo && !citado {
        registros.push(contagens);
    }
    const CANDIDATOS: [char; 3] = [',', ';', '\t'];
    let cabecalho = registros.first().copied().unwrap_or_default();
    if let Some(p) = preferido {
        // O separador do idioma é obrigatório: se não divide o cabeçalho,
        // nada é dividido (nem linhas de baixo com o separador no texto).
        let divide = CANDIDATOS.iter().position(|d| *d == p).map_or(true, |i| cabecalho[i] > 0);
        return if divide || registros.is_empty() { p } else { SEM_SEPARADOR };
    }
    if cabecalho.iter().all(|n| *n == 0) {
        let linhas_dividem = registros.iter().any(|r| r.iter().any(|n| *n > 0));
        return if linhas_dividem { SEM_SEPARADOR } else { ',' };
    }
    let mut melhor = (',', (0usize, false, 0usize));
    for (i, d) in CANDIDATOS.into_iter().enumerate() {
        if cabecalho[i] == 0 {
            continue;
        }
        let mut frequencias = std::collections::HashMap::new();
        for registro in &registros {
            if registro[i] > 0 {
                *frequencias.entry(registro[i]).or_insert(0usize) += 1;
            }
        }
        let score = frequencias
            .into_iter()
            .map(|(n, frequencia)| (frequencia, registros.first().is_some_and(|r| r[i] == n), n))
            .max()
            .unwrap_or_default();
        if score > melhor.1 {
            melhor = (d, score);
        }
    }
    melhor.0
}


/// Escreve um campo CSV/TSV com as mesmas regras de \`toDelimited\` do
/// frontend TypeScript: aspas quando há separador, aspas ou quebra de linha,
/// duplicando as aspas internas. Escreve diretamente no destino (sem
/// materializar um texto auxiliar), inclusive para campos grandes.
pub fn escrever_campo<W: std::io::Write>(
    destino: &mut W,
    campo: &str,
    separador: char,
) -> std::io::Result<()> {
    let citar = campo.chars().any(|c| matches!(c, '"' | '\r' | '\n') || c == separador);
    if !citar {
        return destino.write_all(campo.as_bytes());
    }
    destino.write_all(b"\"")?;
    for (indice, parte) in campo.split('"').enumerate() {
        if indice > 0 {
            destino.write_all(b"\"\"")?;
        }
        destino.write_all(parte.as_bytes())?;
    }
    destino.write_all(b"\"")
}

/// Porta de \`parseDelimitedLine\` do TypeScript. Quando não há aspas,
/// evita alocar a matriz completa; texto vazio ainda produz um campo vazio.
pub fn dividir_csv_linha(linha: &str, separador: char) -> Vec<String> {
    if !linha.contains('"') {
        return linha.split(separador).map(str::to_owned).collect();
    }
    dividir_csv(linha, separador).into_iter().next().unwrap_or_default()
}

/// Porta de \`toDelimited\`: útil para área de transferência e dados pequenos.
/// Para arquivos grandes prefira \`escrever_campo\` em um escritor em fluxo.
pub fn para_delimitado(linhas: &[Vec<String>], separador: char) -> String {
    let mut saida = Vec::new();
    let mut separador_utf8 = [0u8; 4];
    let bytes_separador = separador.encode_utf8(&mut separador_utf8).as_bytes();
    for (i, linha) in linhas.iter().enumerate() {
        if i > 0 {
            saida.push(b'\n');
        }
        for (j, campo) in linha.iter().enumerate() {
            if j > 0 {
                saida.extend_from_slice(bytes_separador);
            }
            // Vec<u8> implementa Write sem operação falível de I/O.
            escrever_campo(&mut saida, campo, separador)
                .expect("escrever em Vec<u8> nao falha");
        }
    }
    String::from_utf8(saida).expect("campos UTF-8 sempre geram UTF-8 valido")
}

/// Lê todos os registros em uma matriz de campos, sem converter números.
/// Prefira `dividir_csv_em_lotes` para arquivos grandes.
pub fn dividir_csv(conteudo: &str, delim: char) -> Vec<Vec<String>> {
    let mut linhas = Vec::new();
    dividir_csv_em_lotes(conteudo, delim, usize::MAX, |lote| linhas.append(lote));
    linhas
}

/// Emite lotes de até `lote` registros, com valores textuais completos.
/// Suporta BOM UTF-8, CRLF/CR, linhas citadas com quebras, delimitadores
/// em aspas e escapes `""`. Um lote zero equivale a uma linha.
/// A callback pode fazer `append` ou processar as linhas sem cópia.
pub fn dividir_csv_em_lotes(
    conteudo: &str,
    delim: char,
    lote: usize,
    mut entregar: impl FnMut(&mut Vec<Vec<String>>),
) {
    let conteudo = conteudo.strip_prefix('\u{feff}').unwrap_or(conteudo);
    let lote = lote.max(1);
    let mut linhas: Vec<Vec<String>> = Vec::with_capacity(lote.min(LINHAS_POR_LOTE));
    let mut linha_atual: Vec<String> = Vec::new();
    let mut campo = String::new();
    let mut entre_aspas = false;
    let mut chars = conteudo.chars().peekable();

    while let Some(c) = chars.next() {
        if entre_aspas {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    campo.push('"');
                    chars.next();
                } else {
                    entre_aspas = false;
                }
            } else {
                campo.push(c);
            }
            continue;
        }
        if c == '"' && campo.is_empty() {
            entre_aspas = true;
            continue;
        }
        if c == delim {
            linha_atual.push(std::mem::take(&mut campo));
            continue;
        }
        if c == '\n' || c == '\r' {
            if c == '\r' && chars.peek() == Some(&'\n') {
                chars.next();
            }
            linha_atual.push(std::mem::take(&mut campo));
            let largura = linha_atual.len();
            linhas.push(std::mem::replace(
                &mut linha_atual,
                Vec::with_capacity(largura),
            ));
            if linhas.len() >= lote {
                entregar(&mut linhas);
                linhas.clear();
            }
            continue;
        }
        campo.push(c);
    }
    if !campo.is_empty() || !linha_atual.is_empty() {
        linha_atual.push(campo);
        linhas.push(linha_atual);
    }
    if !linhas.is_empty() {
        entregar(&mut linhas);
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn porta_serializacao_delimitada_ts_e_roundtrip_sem_perder_unicode() {
        let linhas = vec![
            vec!["Nome".into(), "Observação".into()],
            vec!["João".into(), "um, dois".into()],
            vec!["Ana".into(), "aspas \" e\nquebra".into()],
            vec!["Bia".into(), "".into()],
        ];
        let csv = para_delimitado(&linhas, ',');
        assert_eq!(
            csv,
            "Nome,Observação\nJoão,\"um, dois\"\nAna,\"aspas \"\" e\nquebra\"\nBia,"
        );
        assert_eq!(dividir_csv(&csv, ','), linhas);
        assert_eq!(
            dividir_csv_linha("A;\"B;C\";D", ';'),
            ["A", "B;C", "D"]
        );
        assert_eq!(dividir_csv_linha("", ';'), [""], "linha vazia tem um campo");
    }

    #[test]
    fn porta_to_delimited_ts_tsv_escapa_tab_crlf_e_aspas() {
        let linhas = vec![
            vec!["campo\tcom tab".into(), "aspas \"duplas\"".into()],
            vec!["linha\r\nmúltipla".into(), "ação".into()],
        ];
        let tsv = para_delimitado(&linhas, '\t');
        assert_eq!(
            tsv,
            "\"campo\tcom tab\"\t\"aspas \"\"duplas\"\"\"\n\"linha\r\nmúltipla\"\tação"
        );
        assert_eq!(dividir_csv(&tsv, '\t'), linhas);
        let mut saida = Vec::new();
        escrever_campo(&mut saida, "ação", ',').unwrap();
        assert_eq!(String::from_utf8(saida).unwrap(), "ação");
    }



    #[test]
    fn detecta_consistencia_sem_dividir_separadores_incidentais_ou_citados() {
        for texto in [
            "nome,nota,valor\nAna,texto;extra,1\nJoão,normal,2\nBia,outra;nota;extra,3",
            "nome,nota\nAna,\"muitos;;;;;;;;;;;;;;;;\"\nJoão,\"mais;;;;;;\"",
            "\"nome;;;;\",nota\n\"Ana\n;;;;\",\"texto;extra\"\nJoão,normal",
        ] {
            assert_eq!(detectar_separador(texto), ',');
            let esperado = dividir_csv(texto, ',');
            let mut lido = Vec::new();
            let leitor = std::io::BufReader::with_capacity(1, texto.as_bytes());
            assert_eq!(dividir_csv_leitor(leitor, None, 1, |l| lido.append(l)).unwrap(), ',');
            assert_eq!(lido, esperado);
        }
        assert_eq!(detectar_separador("nome;valor\nAna;1,5\nJoão;2,5\nBia;3,5"), ';');
    }

    #[test]
    fn leitura_incremental_preserva_parser_em_fronteiras_de_buffer() {
        let exemplos = [
            "\u{feff}Nome;Nota\r\nJoão;\"ação\r\nç,;\"\"fim\"\"\"\r\nAna;2\r\n",
            "a;b\r1;2\r3;4",
            "\"x\ny\";z\n;\n",
            "",
            "a;",
            "\"\"",
            "\n\n",
            "a;\"aspas\"x;fim\n",
            "a;\"sem fechar\ncontinua",
        ];
        for texto in exemplos {
            for capacidade in [1, 2, 7, 64] {
                for tamanho in [1, 3] {
                    let leitor = std::io::BufReader::with_capacity(capacidade, texto.as_bytes());
                    let mut resultado = Vec::new();
                    dividir_csv_leitor(leitor, Some(';'), tamanho, |l| resultado.append(l))
                        .unwrap();
                    assert_eq!(
                        resultado,
                        dividir_csv(texto, ';'),
                        "capacidade={capacidade}, texto={texto:?}"
                    );
                }
            }
        }
        let mut resultado = Vec::new();
        assert_eq!(
            dividir_csv_leitor(std::io::Cursor::new("a,b\n1,2"), None, 1, |l| resultado
                .append(l))
            .unwrap(),
            ','
        );
        assert_eq!(resultado, dividir_csv("a,b\n1,2", ','));
        assert!(dividir_csv_leitor(std::io::Cursor::new([b'a', 0xff]), None, 1, |_| {}).is_err());
    }

    #[test]
    fn sep_preserva_heuristica_e_empate_do_sheet() {
        assert_eq!(detectar_separador("a,b;c\n1,2;3"), ',');
        assert_eq!(detectar_separador("a;b;c\n1,5;2;3"), ';');
        assert_eq!(detectar_separador("a\tb\r\n1,2,3"), '\t');
        assert_eq!(detectar_separador("sem separador"), ',');
        assert_eq!(detectar_separador("\u{feff}um;dois;tres\r\n1;2;3"), ';');
    }

    #[test]
    fn o_cabecalho_decide_o_separador() {
        // Idioma pede `;` e o arquivo é com vírgula: uma coluna só, nem a
        // linha de baixo que tem `;` no texto é cortada.
        let virgula = "nome,valor\nAna,1\nJoão;2";
        assert_eq!(escolher_separador(virgula, Some(';')), SEM_SEPARADOR);
        // Cabeçalho com `;` e vírgula decimal embaixo: fica o `;`.
        assert_eq!(escolher_separador("nome;valor\nAna;1,5", Some(';')), ';');
        // Cabeçalho sem separador: não corta as linhas de baixo.
        assert_eq!(detectar_separador("observacao\numa, duas\ntres, quatro"), SEM_SEPARADOR);
        let mut lido = Vec::new();
        dividir_csv_leitor_preferindo(std::io::Cursor::new(virgula), Some(';'), 10, |l| lido.append(l)).unwrap();
        assert_eq!(lido, vec![vec!["nome,valor"], vec!["Ana,1"], vec!["João;2"]]);
        let mut uma = Vec::new();
        dividir_csv_leitor(std::io::Cursor::new("obs\na,b\nc;d"), None, 10, |l| uma.append(l)).unwrap();
        assert_eq!(uma, vec![vec!["obs"], vec!["a,b"], vec!["c;d"]]);
    }

    #[test]
    fn bom_unicode_locale_e_campos_vazios_nao_sao_convertidos() {
        let dados = "\u{feff}Nome;Valor;Nota\r\nJoão;1.234,56;\r\nAna;-0,25;ç\r\n";
        assert_eq!(
            dividir_csv(dados, ';'),
            vec![
                vec!["Nome", "Valor", "Nota"],
                vec!["João", "1.234,56", ""],
                vec!["Ana", "-0,25", "ç"],
            ]
        );
    }

    #[test]
    fn aspas_e_multilinhas_resistem_a_crlf_e_delimitador() {
        let dados = "a;\"b;c\";\"linha\r\ndois\"\r\n\"a\"\"b\";2\r\n";
        assert_eq!(
            dividir_csv(dados, ';'),
            vec![vec!["a", "b;c", "linha\r\ndois"], vec!["a\"b", "2"]]
        );
    }

    #[test]
    fn lotes_reconstroem_exatamente_matriz_incluindo_multilinhas() {
        let dados = "\u{feff}a;\"b;c\"\r\n\"x\ny\";2\r\n1;2\n3;4";
        let esperado = dividir_csv(dados, ';');
        for capacidade in [0, 1, 2, 3, 50, usize::MAX] {
            let mut obtido = Vec::new();
            let mut tamanhos = Vec::new();
            dividir_csv_em_lotes(dados, ';', capacidade, |linhas| {
                tamanhos.push(linhas.len());
                obtido.append(linhas);
            });
            assert_eq!(obtido, esperado, "lote={capacidade}");
            assert!(tamanhos.iter().all(|n| *n > 0 && *n <= capacidade.max(1)));
        }
    }

    #[test]
    fn texto_vazio_ou_so_bom_nao_produz_linhas() {
        assert!(dividir_csv("", ',').is_empty());
        assert!(dividir_csv("\u{feff}", ',').is_empty());
    }
}
