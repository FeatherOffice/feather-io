//! Entrada/saída incremental cooperativa, sem dependência de GUI.
//! O contrato é opt-in: codecs sem implementação de fluxo recusam a operação,
//! em vez de materializar silenciosamente um arquivo inteiro em memória.

use crate::{Celula, Documento, Tabela, Valor};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};

/// Erros exclusivos da nova API incremental. O `ErroIo` legado permanece
/// intacto para não quebrar `match` exaustivo de consumidores antigos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErroFluxo {
    FormatoDesconhecido(String),
    TipoIncompativel,
    DadosInvalidos(String),
    LimiteExcedido,
    Cancelado,
    FluxoNaoSuportado,
    FalhaIo(String),
}

impl From<crate::ErroIo> for ErroFluxo {
    fn from(erro: crate::ErroIo) -> Self {
        match erro {
            crate::ErroIo::FormatoDesconhecido(ext) => Self::FormatoDesconhecido(ext),
            crate::ErroIo::TipoIncompativel => Self::TipoIncompativel,
            crate::ErroIo::DadosInvalidos(descricao) => Self::DadosInvalidos(descricao),
            crate::ErroIo::LimiteExcedido => Self::LimiteExcedido,
        }
    }
}

// Alias interno preserva a semântica dos erros no parser sem mudar sua API.
use self::ErroFluxo as ErroIo;

/// Tetos independentes: bytes de entrada/saída, texto retido e dimensões.
/// Reduzir os limites é recomendado para arquivos de origem não confiável.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LimitesFluxo {
    pub bytes_entrada: u64,
    pub bytes_saida: u64,
    pub bytes_texto: usize,
    pub bytes_campo: usize,
    pub linhas: usize,
    pub colunas: usize,
    pub celulas: usize,
}

impl Default for LimitesFluxo {
    fn default() -> Self {
        Self {
            bytes_entrada: 128 * 1024 * 1024,
            bytes_saida: 128 * 1024 * 1024,
            bytes_texto: 64 * 1024 * 1024,
            bytes_campo: 1024 * 1024,
            linhas: 1_000_000,
            colunas: 16_384,
            celulas: 500_000,
        }
    }
}

/// Progresso medido em bytes realmente lidos/escritos (não percentuais estimados).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgressoFluxo {
    pub bytes_processados: u64,
    pub bytes_totais: Option<u64>,
}

/// Cancelamento cooperativo; o sinal pode ser alterado por outra thread ou
/// pelo callback. Leituras/escritas bloqueantes do SO não são interrompidas.
pub struct ControleFluxo<'a> {
    cancelado: &'a AtomicBool,
    progresso: Option<&'a mut dyn FnMut(ProgressoFluxo)>,
    total: Option<u64>,
}

impl<'a> ControleFluxo<'a> {
    pub fn novo(cancelado: &'a AtomicBool) -> Self {
        Self { cancelado, progresso: None, total: None }
    }

    pub fn com_progresso(
        cancelado: &'a AtomicBool,
        progresso: &'a mut dyn FnMut(ProgressoFluxo),
    ) -> Self {
        Self { cancelado, progresso: Some(progresso), total: None }
    }

    /// Opcional: tamanho conhecido pelo consumidor, sem ler o arquivo inteiro.
    pub fn com_total(mut self, total: u64) -> Self {
        self.total = Some(total);
        self
    }

    fn verificar(&self) -> Result<(), ErroIo> {
        if self.cancelado.load(Ordering::Relaxed) {
            Err(ErroIo::Cancelado)
        } else {
            Ok(())
        }
    }

    fn atualizar(&mut self, bytes: u64) -> Result<(), ErroIo> {
        if let Some(cb) = self.progresso.as_mut() {
            cb(ProgressoFluxo { bytes_processados: bytes, bytes_totais: self.total });
        }
        self.verificar()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Campo {
    Inicio,
    SemAspas,
    EntreAspas,
    Fechado,
}

struct LeitorCsv<'a> {
    limites: &'a LimitesFluxo,
    estado: Campo,
    campo: Vec<u8>,
    dados: Vec<(usize, usize, String)>,
    linhas: usize,
    coluna: usize,
    largura: usize,
    celulas: usize,
    texto_total: usize,
    linha_aberta: bool,
    ignorar_lf: bool,
}

impl<'a> LeitorCsv<'a> {
    fn novo(limites: &'a LimitesFluxo) -> Self {
        Self {
            limites, estado: Campo::Inicio, campo: Vec::new(), dados: Vec::new(),
            linhas: 0, coluna: 0, largura: 0, celulas: 0, texto_total: 0,
            linha_aberta: false, ignorar_lf: false,
        }
    }

    fn adicionar(&mut self, b: u8) -> Result<(), ErroIo> {
        if self.campo.len() >= self.limites.bytes_campo {
            return Err(ErroIo::LimiteExcedido);
        }
        self.campo.push(b);
        Ok(())
    }

    fn concluir_campo(&mut self) -> Result<(), ErroIo> {
        if self.coluna >= self.limites.colunas || self.celulas >= self.limites.celulas {
            return Err(ErroIo::LimiteExcedido);
        }
        self.texto_total = self.texto_total.checked_add(self.campo.len())
            .ok_or(ErroIo::LimiteExcedido)?;
        if self.texto_total > self.limites.bytes_texto {
            return Err(ErroIo::LimiteExcedido);
        }
        let texto = String::from_utf8(std::mem::take(&mut self.campo))
            .map_err(|_| ErroIo::DadosInvalidos("campo CSV contém UTF-8 inválido".into()))?;
        if !texto.is_empty() {
            self.dados.push((self.linhas, self.coluna, texto));
        }
        self.coluna += 1;
        self.celulas += 1;
        self.estado = Campo::Inicio;
        Ok(())
    }

    fn concluir_linha(&mut self) -> Result<(), ErroIo> {
        if self.linhas >= self.limites.linhas {
            return Err(ErroIo::LimiteExcedido);
        }
        self.largura = self.largura.max(self.coluna);
        self.linhas += 1;
        self.coluna = 0;
        self.linha_aberta = false;
        Ok(())
    }

    fn byte(&mut self, b: u8) -> Result<(), ErroIo> {
        if self.ignorar_lf {
            self.ignorar_lf = false;
            if b == b'\n' { return Ok(()); }
        }
        self.linha_aberta = true;
        match self.estado {
            Campo::EntreAspas => {
                if b == b'"' { self.estado = Campo::Fechado; }
                else { self.adicionar(b)?; }
            }
            Campo::Fechado if b == b'"' => {
                self.adicionar(b'"')?;
                self.estado = Campo::EntreAspas;
            }
            Campo::Fechado | Campo::Inicio | Campo::SemAspas => {
                if b == b',' {
                    self.concluir_campo()?;
                } else if b == b'\r' || b == b'\n' {
                    self.concluir_campo()?;
                    self.concluir_linha()?;
                    self.ignorar_lf = b == b'\r';
                } else if self.estado == Campo::Fechado {
                    return Err(ErroIo::DadosInvalidos("caractere após aspas finais no CSV".into()));
                } else if self.estado == Campo::Inicio && b == b'"' {
                    self.estado = Campo::EntreAspas;
                } else if b == b'"' {
                    return Err(ErroIo::DadosInvalidos("aspas não escapadas no CSV".into()));
                } else {
                    self.adicionar(b)?;
                    self.estado = Campo::SemAspas;
                }
            }
        }
        Ok(())
    }

    fn terminar(mut self) -> Result<Documento, ErroIo> {
        if self.estado == Campo::EntreAspas {
            return Err(ErroIo::DadosInvalidos("aspas CSV não fechadas".into()));
        }
        if self.linha_aberta {
            self.concluir_campo()?;
            self.concluir_linha()?;
        }
        let mut tabela = Tabela::nova(self.linhas, self.largura);
        for (linha, coluna, valor) in self.dados {
            tabela.definir(linha, coluna, Celula {
                valor: Valor::Texto(valor), ..Default::default()
            }).map_err(|e| ErroIo::DadosInvalidos(format!("{e:?}")))?;
        }
        Ok(Documento::Tabela(tabela))
    }
}

/// CSV RFC 4180 em leitura incremental, incluindo BOM, campos multiline e
/// UTF-8 fragmentado. A tabela resultante é materializada, mas o arquivo NÃO.
/// Limites são verificados antes de armazenar campos/células.
pub fn ler_csv(
    leitor: &mut dyn Read, limites: &LimitesFluxo, controle: &mut ControleFluxo<'_>,
) -> Result<Documento, ErroIo> {
    controle.verificar()?;
    let mut csv = LeitorCsv::novo(limites);
    let mut buf = [0u8; 8192];
    let mut lidos = 0u64;
    let mut prefixo = Vec::with_capacity(3);
    let mut inicio_decidido = false;
    loop {
        controle.verificar()?;
        let n = match leitor.read(&mut buf) {
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(ErroIo::FalhaIo(e.to_string())),
        };
        if n == 0 { break; }
        lidos = lidos.checked_add(n as u64).ok_or(ErroIo::LimiteExcedido)?;
        if lidos > limites.bytes_entrada { return Err(ErroIo::LimiteExcedido); }
        for (i, &b) in buf[..n].iter().enumerate() {
            if i % 1024 == 0 { controle.verificar()?; }
            if !inicio_decidido {
                prefixo.push(b);
                if prefixo.len() == 3 {
                    if prefixo.as_slice() != &[0xEF, 0xBB, 0xBF] {
                        for v in prefixo.drain(..) { csv.byte(v)?; }
                    }
                    inicio_decidido = true;
                }
            } else {
                csv.byte(b)?;
            }
        }
        controle.atualizar(lidos)?;
    }
    if !inicio_decidido {
        for b in prefixo { csv.byte(b)?; }
    }
    controle.atualizar(lidos)?;
    csv.terminar()
}

struct EscritorLimitado<'a, 'b> {
    escritor: &'a mut dyn Write,
    limites: &'a LimitesFluxo,
    controle: &'a mut ControleFluxo<'b>,
    escritos: u64,
}

impl EscritorLimitado<'_, '_> {
    fn escrever(&mut self, bytes: &[u8]) -> Result<(), ErroIo> {
        // Fragmenta buffers longos para permitir cancelamento cooperativo.
        for bloco in bytes.chunks(8192) {
            self.controle.verificar()?;
            let prox = self.escritos.checked_add(bloco.len() as u64)
                .ok_or(ErroIo::LimiteExcedido)?;
            if prox > self.limites.bytes_saida { return Err(ErroIo::LimiteExcedido); }
            self.escritor.write_all(bloco).map_err(|e| ErroIo::FalhaIo(e.to_string()))?;
            self.escritos = prox;
            self.controle.atualizar(self.escritos)?;
        }
        Ok(())
    }
}

/// Serialização CSV diretamente em `Write`, sem `Vec<u8>` do arquivo completo.
/// Texto de cada célula é temporário e limitado por `bytes_campo`.
pub fn gravar_csv(
    documento: &Documento, escritor: &mut dyn Write,
    limites: &LimitesFluxo, controle: &mut ControleFluxo<'_>,
) -> Result<(), ErroIo> {
    let Documento::Tabela(tabela) = documento else { return Err(ErroIo::TipoIncompativel); };
    controle.verificar()?;
    let (linhas, colunas) = tabela.dimensoes();
    if linhas > limites.linhas || colunas > limites.colunas ||
        linhas.checked_mul(colunas).map_or(true, |n| n > limites.celulas) {
        return Err(ErroIo::LimiteExcedido);
    }
    let mut saida = EscritorLimitado { escritor, limites, controle, escritos: 0 };
    let mut total_texto = 0usize;
    for r in 0..linhas {
        saida.controle.verificar()?;
        if r > 0 { saida.escrever(b"\n")?; }
        for c in 0..colunas {
            if c > 0 { saida.escrever(b",")?; }
            let valor = tabela.obter(r, c).map(|cel| cel.valor.texto()).unwrap_or_default();
            if valor.len() > limites.bytes_campo { return Err(ErroIo::LimiteExcedido); }
            total_texto = total_texto.checked_add(valor.len()).ok_or(ErroIo::LimiteExcedido)?;
            if total_texto > limites.bytes_texto { return Err(ErroIo::LimiteExcedido); }
            let escapar = valor.as_bytes().iter().any(|&b| matches!(b, b',' | b'"' | b'\n' | b'\r'));
            if escapar { saida.escrever(b"\"")?; }
            let bytes = valor.as_bytes();
            let mut inicio = 0;
            for (i, &b) in bytes.iter().enumerate() {
                if b == b'"' {
                    saida.escrever(&bytes[inicio..i])?;
                    saida.escrever(b"\"\"")?;
                    inicio = i + 1;
                }
            }
            saida.escrever(&bytes[inicio..])?;
            if escapar { saida.escrever(b"\"")?; }
        }
    }
    saida.controle.atualizar(saida.escritos)
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::Registro;
    use std::io::{Cursor, Read};

    struct Fragmentado<R>(R);
    impl<R: Read> Read for Fragmentado<R> {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            let n = out.len().min(2);
            self.0.read(&mut out[..n])
        }
    }

    #[test]
    fn fluxo_roundtrip_utf8_fragmentado_bom_aspas_e_crlf() {
        let entrada = b"\xEF\xBB\xBFnome,valor\r\n\"Jo\xC3\xA3o\",\"linha\r\ndo\"\r\n\"a\"\"b\",\r\n";
        let cancelar = AtomicBool::new(false);
        let mut eventos = Vec::new();
        let mut cb = |p| eventos.push(p);
        let mut controle = ControleFluxo::com_progresso(&cancelar, &mut cb).com_total(entrada.len() as u64);
        let reg = Registro::padrao();
        let doc = reg.ler_fluxo("CSV", &mut Fragmentado(Cursor::new(entrada)), &LimitesFluxo::default(), &mut controle).unwrap();
        drop(controle);
        drop(cb);
        assert_eq!(eventos.last().unwrap().bytes_processados, entrada.len() as u64);
        assert_eq!(eventos.last().unwrap().bytes_totais, Some(entrada.len() as u64));
        let mut saida = Vec::new();
        let mut controle = ControleFluxo::novo(&cancelar);
        reg.gravar_fluxo("csv", &doc, &mut saida, &LimitesFluxo::default(), &mut controle).unwrap();
        let doc2 = reg.ler_fluxo(".csv", &mut Cursor::new(saida), &LimitesFluxo::default(), &mut controle).unwrap();
        assert_eq!(doc, doc2);
    }

    #[test]
    fn limites_rejeitam_alocacao_e_saida() {
        let cancelar = AtomicBool::new(false);
        let mut controle = ControleFluxo::novo(&cancelar);
        let mut limites = LimitesFluxo { celulas: 2, ..Default::default() };
        assert_eq!(ler_csv(&mut Cursor::new(b"a,b,c"), &limites, &mut controle), Err(ErroIo::LimiteExcedido));
        limites.celulas = 100;
        limites.bytes_campo = 2;
        assert_eq!(ler_csv(&mut Cursor::new(b"abc"), &limites, &mut controle), Err(ErroIo::LimiteExcedido));
        limites.bytes_campo = 10;
        limites.bytes_entrada = 2;
        assert_eq!(ler_csv(&mut Cursor::new(b"abc"), &limites, &mut controle), Err(ErroIo::LimiteExcedido));
        limites.bytes_entrada = 100;
        limites.bytes_saida = 1;
        let doc = ler_csv(&mut Cursor::new(b"ab"), &limites, &mut controle).unwrap();
        assert_eq!(gravar_csv(&doc, &mut Vec::new(), &limites, &mut controle), Err(ErroIo::LimiteExcedido));
    }

    #[test]
    fn cancelamento_durante_leitura_progresso() {
        let cancelar = AtomicBool::new(false);
        let mut cb = |p: ProgressoFluxo| { if p.bytes_processados >= 8192 { cancelar.store(true, Ordering::Relaxed); } };
        let mut controle = ControleFluxo::com_progresso(&cancelar, &mut cb);
        let entrada = vec![b'a'; 30_000];
        assert_eq!(ler_csv(&mut Cursor::new(entrada), &LimitesFluxo::default(), &mut controle), Err(ErroIo::Cancelado));
    }

    #[test]
    fn formatos_sem_adaptador_de_fluxo_sao_recusados() {
        let cancelar = AtomicBool::new(false);
        let mut controle = ControleFluxo::novo(&cancelar);
        let reg = Registro::padrao();

        // PNG é registrado para exportação em memória, mas não possui
        // leitor/escritor incremental. Recusar sem consumir bytes.
        let mut entrada = Cursor::new(b"conteudo nao consumido");
        assert_eq!(
            reg.ler_fluxo("png", &mut entrada, &LimitesFluxo::default(), &mut controle),
            Err(ErroIo::FluxoNaoSuportado)
        );
        assert_eq!(entrada.position(), 0);

        // Formato realmente ausente tem diagnóstico diferente.
        let mut desconhecido = Cursor::new(b"entrada desconhecida");
        assert_eq!(
            reg.ler_fluxo("formato_inexistente", &mut desconhecido, &LimitesFluxo::default(), &mut controle),
            Err(ErroIo::FormatoDesconhecido("formato_inexistente".into()))
        );
        assert_eq!(desconhecido.position(), 0);

        let imagem = Documento::Imagem(
            feather_image::ImagemRgba::nova(1, 1, vec![1, 2, 3, 255]).unwrap()
        );
        let mut saida = Vec::new();
        assert_eq!(
            reg.gravar_fluxo("png", &imagem, &mut saida, &LimitesFluxo::default(), &mut controle),
            Err(ErroIo::FluxoNaoSuportado)
        );
        assert!(saida.is_empty());
    }

    #[test]
    fn cancelamento_interrompe_gravacao_apos_bloco() {
        let registro = Registro::padrao();
        let entrada = vec![b'a'; 30_000];
        let documento = registro.ler("csv", &entrada).unwrap();
        let cancelar = AtomicBool::new(false);
        let mut callback = |p: ProgressoFluxo| {
            if p.bytes_processados >= 8192 {
                cancelar.store(true, Ordering::Relaxed);
            }
        };
        let mut controle = ControleFluxo::com_progresso(&cancelar, &mut callback);
        let mut destino = Vec::new();
        assert_eq!(
            registro.gravar_fluxo("csv", &documento, &mut destino, &LimitesFluxo::default(), &mut controle),
            Err(ErroIo::Cancelado)
        );
        assert_eq!(destino.len(), 8192);
    }

    #[test]
    fn codigo_legado_sem_streaming_recusa_sem_consumir_entrada() {
        struct Legado;
        impl crate::Codec for Legado {
            fn extensoes(&self) -> &'static [&'static str] { &["legado"] }
            fn ler(&self, _bytes: &[u8]) -> Result<Documento, crate::ErroIo> {
                Ok(Documento::Tabela(Tabela::nova(0, 0)))
            }
            fn gravar(&self, _d: &Documento) -> Result<Vec<u8>, crate::ErroIo> { Ok(Vec::new()) }
        }
        let mut registro = Registro::padrao();
        registro.registrar(Legado);
        let cancelar = AtomicBool::new(false);
        let mut controle = ControleFluxo::novo(&cancelar);
        let mut entrada = Cursor::new(b"sem consumo");
        assert_eq!(
            registro.ler_fluxo("legado", &mut entrada, &LimitesFluxo::default(), &mut controle),
            Err(ErroIo::FluxoNaoSuportado)
        );
        assert_eq!(entrada.position(), 0);
    }

    #[test]
    fn entradas_invalidas_e_vazias() {
        let cancelar = AtomicBool::new(false);
        let mut controle = ControleFluxo::novo(&cancelar);
        assert!(matches!(ler_csv(&mut Cursor::new(b"\"sem fim"), &LimitesFluxo::default(), &mut controle), Err(ErroIo::DadosInvalidos(_))));
        assert!(matches!(ler_csv(&mut Cursor::new(b"\xff"), &LimitesFluxo::default(), &mut controle), Err(ErroIo::DadosInvalidos(_))));
        let doc = ler_csv(&mut Cursor::new(b""), &LimitesFluxo::default(), &mut controle).unwrap();
        let Documento::Tabela(t) = doc else { panic!("esperada tabela"); };
        assert_eq!(t.dimensoes(), (0, 0));
    }
}