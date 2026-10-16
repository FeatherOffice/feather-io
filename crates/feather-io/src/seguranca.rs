//! Operações defensivas de arquivos sem dependência de GUI nem descompactador.
//! A inspeção ZIP é uma pré-condição, não substitui limites durante a extração.

use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErroSeguranca {
    EntradaInvalida(&'static str),
    LimiteExcedido(&'static str),
    FalhaIo { operacao: &'static str, detalhe: String },
}
impl fmt::Display for ErroSeguranca {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EntradaInvalida(m) => write!(f, "Arquivo inválido ou potencialmente inseguro: {m}"),
            Self::LimiteExcedido(m) => write!(f, "Arquivo excede limite de segurança: {m}"),
            Self::FalhaIo { operacao, detalhe } => write!(f, "Não foi possível {operacao}: {detalhe}"),
        }
    }
}
impl std::error::Error for ErroSeguranca {}
fn erro_io(op: &'static str, err: std::io::Error) -> ErroSeguranca {
    ErroSeguranca::FalhaIo { operacao: op, detalhe: err.to_string() }
}

/// Limites conservadores, independentes de formatos proprietários.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoliticaArquivos {
    pub max_bytes_arquivo: u64,
    pub max_entradas_zip: usize,
    pub max_bytes_expandido: u64,
    pub max_bytes_entrada_expandida: u64,
    pub max_razao_expansao: u64,
    pub max_bytes_caminho: usize,
}
impl Default for PoliticaArquivos {
    fn default() -> Self {
        Self {
            max_bytes_arquivo: 128 * 1024 * 1024,
            max_entradas_zip: 10_000,
            max_bytes_expandido: 512 * 1024 * 1024,
            max_bytes_entrada_expandida: 128 * 1024 * 1024,
            max_razao_expansao: 200,
            max_bytes_caminho: 1024,
        }
    }
}

/// Orçamento para consumo REAL de bytes descompactados em qualquer profundidade.
/// Deve ser compartilhado entre as entradas e os contêineres aninhados.
#[derive(Debug, Clone)]
pub struct OrcamentoExtracao {
    limite: u64,
    consumido: u64,
}
impl OrcamentoExtracao {
    pub fn novo(limite: u64) -> Self { Self { limite, consumido: 0 } }
    pub fn registrar(&mut self, bytes: usize) -> Result<(), ErroSeguranca> {
        let novo = self.consumido.checked_add(bytes as u64)
            .ok_or(ErroSeguranca::LimiteExcedido("soma descompactada excedeu o inteiro"))?;
        if novo > self.limite { return Err(ErroSeguranca::LimiteExcedido("bytes descompactados")); }
        self.consumido = novo;
        Ok(())
    }
    pub fn consumido(&self) -> u64 { self.consumido }
    pub fn restante(&self) -> u64 { self.limite - self.consumido }
}

/// Lê para um buffer limitado e só entrega o buffer quando a leitura termina.
/// Um arquivo alterado in-place por terceiros durante a leitura não é um snapshot
/// transacional; o consumidor deve coordenar o acesso ao conteúdo.
pub fn ler_arquivo_limitado(caminho: &Path, politica: &PoliticaArquivos) -> Result<Vec<u8>, ErroSeguranca> {
    let mut arquivo = File::open(caminho).map_err(|e| erro_io("abrir arquivo", e))?;
    let antes = arquivo.metadata().map_err(|e| erro_io("consultar tamanho", e))?;
    if antes.len() > politica.max_bytes_arquivo {
        return Err(ErroSeguranca::LimiteExcedido("tamanho do arquivo"));
    }
    let mut bytes = Vec::new();
    let mut bloco = [0u8; 8192];
    loop {
        let n = match arquivo.read(&mut bloco) {
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(erro_io("ler arquivo", e)),
        };
        if n == 0 { break; }
        let novo = (bytes.len() as u64).checked_add(n as u64)
            .ok_or(ErroSeguranca::LimiteExcedido("tamanho do arquivo"))?;
        if novo > politica.max_bytes_arquivo { return Err(ErroSeguranca::LimiteExcedido("tamanho do arquivo")); }
        bytes.try_reserve(n).map_err(|_| ErroSeguranca::LimiteExcedido("memória de leitura"))?;
        bytes.extend_from_slice(&bloco[..n]);
    }
    let depois = arquivo.metadata().map_err(|e| erro_io("reverificar arquivo", e))?;
    if antes.len() != depois.len() || depois.len() != bytes.len() as u64 ||
        antes.modified().ok() != depois.modified().ok() {
        return Err(ErroSeguranca::EntradaInvalida("arquivo foi alterado durante a leitura"));
    }
    Ok(bytes)
}

struct SaidaLimitada<'a> { arquivo: &'a mut File, maximo: u64, escritos: u64, limite_violado: bool }
impl Write for SaidaLimitada<'_> {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        let restante = self.maximo.saturating_sub(self.escritos);
        if (buffer.len() as u64) > restante {
            self.limite_violado = true;
            return Err(std::io::Error::other("limite de bytes gravados excedido"));
        }
        let n = self.arquivo.write(buffer)?;
        self.escritos += n as u64;
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> { self.arquivo.flush() }
}
struct ArquivoTemporario(PathBuf);
impl Drop for ArquivoTemporario {
    fn drop(&mut self) { let _ = fs::remove_file(&self.0); }
}
static SEQUENCIA: AtomicU64 = AtomicU64::new(0);

/// Grava em arquivo irmão exclusivo, sincroniza os bytes e renomeia somente
/// em caso de sucesso. Em erro o destino anterior é preservado e o tmp removido.
/// A atomicidade do rename depende do sistema de arquivos local; não suporta
/// garantias sobre pastas atacáveis nem substitui locking entre escritores.
pub fn gravar_atomico<F>(destino: &Path, politica: &PoliticaArquivos, escrever: F) -> Result<(), ErroSeguranca>
where F: FnOnce(&mut dyn Write) -> Result<(), ErroSeguranca> {
    let pasta = destino.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
    let nome = destino.file_name().ok_or(ErroSeguranca::EntradaInvalida("destino não é arquivo"))?;
    let instante = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    let mut temporario = None;
    for _ in 0..32 {
        let numero = SEQUENCIA.fetch_add(1, Ordering::Relaxed);
        let mut nome_tmp = std::ffi::OsString::from(".");
        nome_tmp.push(nome);
        nome_tmp.push(format!(".{}-{instante}-{numero}.tmp", std::process::id()));
        let caminho = pasta.join(nome_tmp);
        match OpenOptions::new().write(true).create_new(true).open(&caminho) {
            Ok(f) => { temporario = Some((ArquivoTemporario(caminho), f)); break; }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(erro_io("criar arquivo temporário", e)),
        }
    }
    let (guard, mut arquivo) = temporario.ok_or(ErroSeguranca::EntradaInvalida("não foi possível reservar temporário"))?;
    {
        let mut saida = SaidaLimitada { arquivo: &mut arquivo, maximo: politica.max_bytes_arquivo, escritos: 0, limite_violado: false };
        escrever(&mut saida)?;
        if saida.limite_violado { return Err(ErroSeguranca::LimiteExcedido("tamanho de saída")); }
        saida.flush().map_err(|e| erro_io("esvaziar buffer de gravação", e))?;
    }
    arquivo.sync_all().map_err(|e| erro_io("sincronizar arquivo temporário", e))?;
    drop(arquivo);
    fs::rename(&guard.0, destino).map_err(|e| erro_io("substituir arquivo original", e))?;
    // Best-effort: sincronizar o diretório em Unix após o rename.
    #[cfg(unix)]
    { if let Ok(pasta) = File::open(pasta) { let _ = pasta.sync_all(); } }
    Ok(())
}

pub fn gravar_bytes_atomico(destino: &Path, bytes: &[u8], politica: &PoliticaArquivos) -> Result<(), ErroSeguranca> {
    if (bytes.len() as u64) > politica.max_bytes_arquivo {
        return Err(ErroSeguranca::LimiteExcedido("tamanho de saída"));
    }
    gravar_atomico(destino, politica, |w| w.write_all(bytes).map_err(|e| erro_io("gravar bytes", e)))
}

fn u16_em(bytes: &[u8], indice: usize) -> Result<u16, ErroSeguranca> {
    let pedaco = bytes.get(indice..indice.checked_add(2).ok_or(ErroSeguranca::EntradaInvalida("overflow de índice"))?)
        .ok_or(ErroSeguranca::EntradaInvalida("ZIP truncado"))?;
    Ok(u16::from_le_bytes([pedaco[0], pedaco[1]]))
}
fn u32_em(bytes: &[u8], indice: usize) -> Result<u32, ErroSeguranca> {
    let pedaco = bytes.get(indice..indice.checked_add(4).ok_or(ErroSeguranca::EntradaInvalida("overflow de índice"))?)
        .ok_or(ErroSeguranca::EntradaInvalida("ZIP truncado"))?;
    Ok(u32::from_le_bytes([pedaco[0], pedaco[1], pedaco[2], pedaco[3]]))
}
fn caminho_seguro(nome: &[u8], maximo: usize) -> Result<(), ErroSeguranca> {
    if nome.is_empty() || nome.len() > maximo { return Err(ErroSeguranca::LimiteExcedido("caminho de entrada ZIP")); }
    let caminho = std::str::from_utf8(nome).map_err(|_| ErroSeguranca::EntradaInvalida("nome ZIP não UTF-8"))?;
    if caminho.starts_with('/') || caminho.contains('\\') || caminho.contains(':') ||
        caminho.chars().any(char::is_control) {
        return Err(ErroSeguranca::EntradaInvalida("ZIP contém caminho inseguro"));
    }
    let sem_barra = caminho.strip_suffix('/').unwrap_or(caminho);
    if sem_barra.split('/').any(|s| s.is_empty() || s == "." || s == "..") {
        return Err(ErroSeguranca::EntradaInvalida("ZIP contém path traversal"));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResumoZip {
    pub entradas: usize,
    pub bytes_compactados: u64,
    pub bytes_expandidos_declarados: u64,
}

/// Valida cabeçalhos locais e diretório central de ZIP convencional, sem
/// descompactar nem confiar só na extensão. Rejeita ZIP64, arquivos divididos,
/// criptografia, data descriptors e métodos que não sejam stored/deflate.
/// Tamanhos declarados podem mentir: usar OrcamentoExtracao ao descompactar.
pub fn validar_zip(bytes: &[u8], politica: &PoliticaArquivos) -> Result<ResumoZip, ErroSeguranca> {
    if bytes.len() as u64 > politica.max_bytes_arquivo {
        return Err(ErroSeguranca::LimiteExcedido("tamanho comprimido"));
    }
    if bytes.len() < 22 { return Err(ErroSeguranca::EntradaInvalida("ZIP sem fim de diretório")); }
    let inicio = bytes.len().saturating_sub(65_557);
    let eocd = (inicio..=bytes.len()-22).rev().find(|&i| {
        bytes.get(i..i+4) == Some(&b"PK\x05\x06"[..]) &&
        u16_em(bytes, i+20).ok().is_some_and(|t| i + 22 + t as usize == bytes.len())
    }).ok_or(ErroSeguranca::EntradaInvalida("diretório ZIP ausente/truncado"))?;
    if u16_em(bytes, eocd+4)? != 0 || u16_em(bytes, eocd+6)? != 0 {
        return Err(ErroSeguranca::EntradaInvalida("ZIP multivolume não suportado"));
    }
    let por_volume = u16_em(bytes, eocd+8)? as usize;
    let quantidade = u16_em(bytes, eocd+10)? as usize;
    if quantidade == 0xffff { return Err(ErroSeguranca::EntradaInvalida("ZIP64 requer validador especializado")); }
    if quantidade != por_volume { return Err(ErroSeguranca::EntradaInvalida("contagem ZIP contraditória")); }
    if quantidade > politica.max_entradas_zip { return Err(ErroSeguranca::LimiteExcedido("entradas ZIP")); }
    let tamanho_dir = u32_em(bytes, eocd+12)? as usize;
    let inicio_dir = u32_em(bytes, eocd+16)? as usize;
    if tamanho_dir == u32::MAX as usize || inicio_dir == u32::MAX as usize {
        return Err(ErroSeguranca::EntradaInvalida("ZIP64 não suportado"));
    }
    if inicio_dir.checked_add(tamanho_dir) != Some(eocd) {
        return Err(ErroSeguranca::EntradaInvalida("diretório ZIP fora de posição"));
    }
    let mut cursor = inicio_dir;
    let mut soma_descompactado = 0u64;
    let mut soma_compactado = 0u64;
    let mut faixas = Vec::with_capacity(quantidade);
    let mut nomes = std::collections::HashSet::with_capacity(quantidade);
    for _ in 0..quantidade {
        let cabecalho = cursor.checked_add(46).ok_or(ErroSeguranca::EntradaInvalida("overflow ZIP"))?;
        if cabecalho > eocd || bytes.get(cursor..cursor+4) != Some(&b"PK\x01\x02"[..]) {
            return Err(ErroSeguranca::EntradaInvalida("entrada de diretório central inválida"));
        }
        let flags = u16_em(bytes, cursor+8)?;
        let metodo = u16_em(bytes, cursor+10)?;
        if flags & 0x0009 != 0 { return Err(ErroSeguranca::EntradaInvalida("ZIP criptografado ou com data descriptor")); }
        if metodo != 0 && metodo != 8 { return Err(ErroSeguranca::EntradaInvalida("método ZIP não suportado")); }
        let compactados = u32_em(bytes, cursor+20)? as u64;
        let expandidos = u32_em(bytes, cursor+24)? as u64;
        if compactados == u32::MAX as u64 || expandidos == u32::MAX as u64 {
            return Err(ErroSeguranca::EntradaInvalida("entrada ZIP64 não suportada"));
        }
        let tamanho_nome = u16_em(bytes, cursor+28)? as usize;
        let extra = u16_em(bytes, cursor+30)? as usize;
        let comentario = u16_em(bytes, cursor+32)? as usize;
        if u16_em(bytes, cursor+34)? != 0 { return Err(ErroSeguranca::EntradaInvalida("entrada ZIP em volume externo")); }
        let modo = u32_em(bytes, cursor+38)? >> 16;
        if modo & 0o170000 == 0o120000 { return Err(ErroSeguranca::EntradaInvalida("link simbólico no ZIP")); }
        let local = u32_em(bytes, cursor+42)? as usize;
        if local == u32::MAX as usize { return Err(ErroSeguranca::EntradaInvalida("offset ZIP64 não suportado")); }
        let nome_inicial = cabecalho;
        let fim = nome_inicial.checked_add(tamanho_nome).and_then(|n| n.checked_add(extra)).and_then(|n| n.checked_add(comentario))
            .ok_or(ErroSeguranca::EntradaInvalida("overflow de entrada ZIP"))?;
        if fim > eocd { return Err(ErroSeguranca::EntradaInvalida("diretório ZIP truncado")); }
        let nome = &bytes[nome_inicial..nome_inicial+tamanho_nome];
        caminho_seguro(nome, politica.max_bytes_caminho)?;
        if !nomes.insert(nome) { return Err(ErroSeguranca::EntradaInvalida("entrada ZIP duplicada")); }
        if expandidos > politica.max_bytes_entrada_expandida {
            return Err(ErroSeguranca::LimiteExcedido("tamanho de entrada ZIP"));
        }
        if (compactados == 0 && expandidos != 0) ||
            (compactados > 0 && expandidos > compactados.saturating_mul(politica.max_razao_expansao)) {
            return Err(ErroSeguranca::LimiteExcedido("razão de expansão ZIP"));
        }
        if metodo == 0 && compactados != expandidos { return Err(ErroSeguranca::EntradaInvalida("entrada ZIP stored com tamanhos inconsistentes")); }
        soma_compactado = soma_compactado.checked_add(compactados).ok_or(ErroSeguranca::LimiteExcedido("soma compactada"))?;
        soma_descompactado = soma_descompactado.checked_add(expandidos).ok_or(ErroSeguranca::LimiteExcedido("soma expandida"))?;
        if soma_descompactado > politica.max_bytes_expandido { return Err(ErroSeguranca::LimiteExcedido("soma expandida ZIP")); }
        let tamanho_local = local.checked_add(30).ok_or(ErroSeguranca::EntradaInvalida("overflow de cabeçalho local"))?;
        if tamanho_local > inicio_dir || bytes.get(local..local+4) != Some(&b"PK\x03\x04"[..]) {
            return Err(ErroSeguranca::EntradaInvalida("cabeçalho ZIP local ausente"));
        }
        if u16_em(bytes, local+6)? != flags || u16_em(bytes, local+8)? != metodo ||
            u32_em(bytes, local+14)? != u32_em(bytes, cursor+16)? ||
            u32_em(bytes, local+18)? as u64 != compactados || u32_em(bytes, local+22)? as u64 != expandidos {
            return Err(ErroSeguranca::EntradaInvalida("ZIP local não confere com diretório"));
        }
        let nome_local = u16_em(bytes, local+26)? as usize;
        let extra_local = u16_em(bytes, local+28)? as usize;
        let inicio_dados = tamanho_local.checked_add(nome_local).and_then(|n| n.checked_add(extra_local))
            .ok_or(ErroSeguranca::EntradaInvalida("overflow ZIP local"))?;
        let fim_dados = inicio_dados.checked_add(compactados as usize).ok_or(ErroSeguranca::EntradaInvalida("overflow de payload ZIP"))?;
        if fim_dados > inicio_dir || bytes.get(tamanho_local..tamanho_local+nome_local) != Some(nome) {
            return Err(ErroSeguranca::EntradaInvalida("payload ou nome ZIP local inválido"));
        }
        faixas.push((local, fim_dados));
        cursor = fim;
    }
    if cursor != eocd { return Err(ErroSeguranca::EntradaInvalida("diretório central contém dados residuais")); }
    faixas.sort_unstable();
    if faixas.windows(2).any(|v| v[0].1 > v[1].0) { return Err(ErroSeguranca::EntradaInvalida("cabeçalhos ou arquivos ZIP sobrepostos")); }
    Ok(ResumoZip { entradas: quantidade, bytes_compactados: soma_compactado, bytes_expandidos_declarados: soma_descompactado })
}

/// Faz a leitura limitada e a pré-validação em uma única operação.
/// Os bytes só são entregues após a inspeção terminar sem erros.
pub fn ler_zip_verificado(caminho: &Path, politica: &PoliticaArquivos) -> Result<(Vec<u8>, ResumoZip), ErroSeguranca> {
    let bytes = ler_arquivo_limitado(caminho, politica)?;
    let resumo = validar_zip(&bytes, politica)?;
    Ok((bytes, resumo))
}

#[cfg(test)]
mod testes {
    use super::*;

    fn empurrar16(buf: &mut Vec<u8>, n: u16) { buf.extend_from_slice(&n.to_le_bytes()); }
    fn empurrar32(buf: &mut Vec<u8>, n: u32) { buf.extend_from_slice(&n.to_le_bytes()); }
    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = !0u32;
        for &byte in bytes {
            crc ^= byte as u32;
            for _ in 0..8 {
                crc = (crc >> 1) ^ (0xedb8_8320u32 & (0u32.wrapping_sub(crc & 1)));
            }
        }
        !crc
    }
    fn zip_simples(nome: &str, conteudo: &[u8]) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend(b"PK\x03\x04"); empurrar16(&mut b, 20); empurrar16(&mut b, 0); empurrar16(&mut b, 0);
        empurrar16(&mut b, 0); empurrar16(&mut b, 0); empurrar32(&mut b, crc32(conteudo));
        empurrar32(&mut b, conteudo.len() as u32); empurrar32(&mut b, conteudo.len() as u32);
        empurrar16(&mut b, nome.len() as u16); empurrar16(&mut b, 0);
        b.extend(nome.as_bytes()); b.extend(conteudo);
        let central = b.len();
        b.extend(b"PK\x01\x02"); empurrar16(&mut b, 20); empurrar16(&mut b, 20);
        empurrar16(&mut b, 0); empurrar16(&mut b, 0); empurrar16(&mut b, 0); empurrar16(&mut b, 0);
        empurrar32(&mut b, crc32(conteudo)); empurrar32(&mut b, conteudo.len() as u32);
        empurrar32(&mut b, conteudo.len() as u32); empurrar16(&mut b, nome.len() as u16);
        empurrar16(&mut b, 0); empurrar16(&mut b, 0); empurrar16(&mut b, 0);
        empurrar16(&mut b, 0); empurrar32(&mut b, 0); empurrar32(&mut b, 0);
        b.extend(nome.as_bytes());
        let tamanho = b.len() - central;
        b.extend(b"PK\x05\x06"); empurrar16(&mut b, 0); empurrar16(&mut b, 0);
        empurrar16(&mut b, 1); empurrar16(&mut b, 1);
        empurrar32(&mut b, tamanho as u32); empurrar32(&mut b, central as u32); empurrar16(&mut b, 0);
        b
    }
    fn pasta_teste() -> PathBuf {
        let seq = SEQUENCIA.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("featheroffice-io012-{}-{seq}", std::process::id()));
        fs::create_dir(&dir).unwrap();
        dir
    }
    #[test]
    fn arquivo_atomico_preserva_destino_em_erro() {
        let dir = pasta_teste(); let destino = dir.join("dados.csv");
        fs::write(&destino, b"original").unwrap();
        let err = gravar_atomico(&destino, &PoliticaArquivos::default(), |w| {
            w.write_all(b"incompleto").unwrap();
            Err(ErroSeguranca::EntradaInvalida("falha proposital"))
        }).unwrap_err();
        assert!(matches!(err, ErroSeguranca::EntradaInvalida(_)));
        assert_eq!(fs::read(&destino).unwrap(), b"original");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn erro_de_tamanho_nao_pode_ser_ignorado_pelo_callback() {
        let dir = pasta_teste(); let destino = dir.join("safe.txt");
        fs::write(&destino, b"antigo").unwrap();
        let p = PoliticaArquivos { max_bytes_arquivo: 2, ..Default::default() };
        let resultado = gravar_atomico(&destino, &p, |w| { let _ = w.write_all(b"1234"); Ok(()) });
        assert!(matches!(resultado, Err(ErroSeguranca::LimiteExcedido(_))));
        assert_eq!(fs::read(&destino).unwrap(), b"antigo");
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn gravação_atomica_com_leitura_limitada() {
        let dir = pasta_teste(); let destino = dir.join("a.txt");
        let mut p = PoliticaArquivos::default(); p.max_bytes_arquivo = 4;
        gravar_bytes_atomico(&destino, b"abcd", &p).unwrap();
        assert_eq!(ler_arquivo_limitado(&destino, &p).unwrap(), b"abcd");
        assert!(matches!(gravar_bytes_atomico(&destino, b"abcde", &p), Err(ErroSeguranca::LimiteExcedido(_))));
        assert_eq!(fs::read(&destino).unwrap(), b"abcd");
        fs::write(&destino, b"abcde").unwrap();
        assert!(matches!(ler_arquivo_limitado(&destino, &p), Err(ErroSeguranca::LimiteExcedido(_))));
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn limite_de_escrita_streaming_preserva_original() {
        let dir = pasta_teste(); let destino = dir.join("b.bin");
        fs::write(&destino, b"old").unwrap();
        let p = PoliticaArquivos { max_bytes_arquivo: 2, ..Default::default() };
        assert!(gravar_atomico(&destino, &p, |w| w.write_all(b"123").map_err(|e| erro_io("escrever", e))).is_err());
        assert_eq!(fs::read(&destino).unwrap(), b"old");
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn crc_das_fixtures_confere_com_valor_conhecido() {
        assert_eq!(crc32(b"teste"), 0xe6b4490f);
    }
    #[test]
    fn zip_minimo_aceito_e_bombas_rejeitadas() {
        let mut zip = zip_simples("word/document.xml", b"teste");
        let policy = PoliticaArquivos::default();
        assert_eq!(validar_zip(&zip, &policy).unwrap().entradas, 1);
        let inicio = zip.len()-22;
        let diretorio = u32_em(&zip, inicio+16).unwrap() as usize;
        zip[diretorio+24..diretorio+28].copy_from_slice(&(100_000_000u32).to_le_bytes());
        assert!(matches!(validar_zip(&zip, &policy), Err(ErroSeguranca::LimiteExcedido(_))));
    }
    #[test]
    fn zip_rejeita_path_traversal() {
        for nome in ["../evil", "/etc/passwd", "a/../../b", "C:/Windows", "dir\\file", "a//b"] {
            assert!(validar_zip(&zip_simples(nome, b"x"), &PoliticaArquivos::default()).is_err(), "{nome}");
        }
        assert!(validar_zip(&zip_simples("word/document.xml", b"x"), &PoliticaArquivos::default()).is_ok());
    }
    #[test]
    fn zip_truncado_e_falso_diretorio_falham() {
        let mut b = zip_simples("a.txt", b"x"); b.truncate(b.len()-4);
        assert!(validar_zip(&b, &PoliticaArquivos::default()).is_err());
        assert!(validar_zip(b"PK\x03\x04abc", &PoliticaArquivos::default()).is_err());
        let mut b = zip_simples("a.txt", b"x"); let n = b.len();
        b[n-6..n-2].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(validar_zip(&b, &PoliticaArquivos::default()).is_err());
    }
    #[test]
    fn zip_razao_e_contagem_limites() {
        let b = zip_simples("a.txt", b"hello");
        let p = PoliticaArquivos { max_entradas_zip: 0, ..Default::default() };
        assert!(matches!(validar_zip(&b, &p), Err(ErroSeguranca::LimiteExcedido(_))));
        let p = PoliticaArquivos { max_bytes_arquivo: 10, ..Default::default() };
        assert!(matches!(validar_zip(&b, &p), Err(ErroSeguranca::LimiteExcedido(_))));
    }
    #[test]
    fn orcamento_real_nao_permite_estouro() {
        let mut b = OrcamentoExtracao::novo(5);
        b.registrar(3).unwrap(); b.registrar(2).unwrap();
        assert_eq!(b.consumido(), 5);
        assert_eq!(b.restante(), 0);
        assert!(matches!(b.registrar(1), Err(ErroSeguranca::LimiteExcedido(_))));
        assert_eq!(b.consumido(), 5);
    }
    #[test]
    fn leitura_zip_verificada_so_entrega_zip_valido() {
        let dir = pasta_teste();
        let destino = dir.join("documento.fdoz");
        fs::write(&destino, zip_simples("word/document.xml", b"ok")).unwrap();
        let (bytes, resumo) = ler_zip_verificado(&destino, &PoliticaArquivos::default()).unwrap();
        assert_eq!(resumo.entradas, 1);
        assert_eq!(validar_zip(&bytes, &PoliticaArquivos::default()).unwrap(), resumo);
        fs::write(&destino, b"PK\x03\x04invalid").unwrap();
        assert!(ler_zip_verificado(&destino, &PoliticaArquivos::default()).is_err());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn zip_nao_torna_png_valido_so_pelo_nome() {
        let b = zip_simples("word/document.xml", "olá".as_bytes());
        assert!(validar_zip(&b, &PoliticaArquivos::default()).is_ok());
        let mut ruim = b.clone(); ruim[0] = 0;
        assert!(validar_zip(&ruim, &PoliticaArquivos::default()).is_err());
    }
    #[test]
    fn aceitar_entrada_vazia_com_diretorio_sem_itens() {
        let mut b = Vec::new(); b.extend(b"PK\x05\x06");
        for _ in 0..9 { empurrar16(&mut b, 0); }
        b.truncate(22);
        assert_eq!(validar_zip(&b, &PoliticaArquivos::default()).unwrap().entradas, 0);
    }
    #[test]
    fn mensagens_sao_legiveis_e_nao_perdem_causa() {
        let erro = ErroSeguranca::LimiteExcedido("soma expandida ZIP");
        assert!(erro.to_string().contains("soma expandida ZIP"));
        let erro = ErroSeguranca::FalhaIo { operacao: "ler arquivo", detalhe: "permissão negada".into() };
        assert!(erro.to_string().contains("permissão negada"));
    }
}