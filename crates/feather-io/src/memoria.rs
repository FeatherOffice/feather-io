//! Limite de memória RAM da suíte: uma vigia, não um teto rígido.
//!
//! O aplicativo informa o limite escolhido nas Configurações
//! ([`definir_limite_gb`]); leitores e operações longas perguntam
//! [`verificar`] entre blocos de trabalho. Passou do limite, a operação
//! para com uma mensagem clara e o programa continua aberto.
//!
//! Por que não um teto do sistema (`setrlimit`, job object): com ele, a
//! primeira alocação acima do limite derruba o processo e o trabalho não
//! salvo se perde. A vigia deixa trabalhar até o limite real de memória,
//! em vez de cortar por contagem de células ou de linhas.

use std::sync::atomic::{AtomicU64, Ordering};

/// Limite de fábrica, em GB.
pub const LIMITE_PADRAO_GB: u16 = 6;
pub const LIMITE_MIN_GB: u16 = 2;
pub const LIMITE_MAX_GB: u16 = 128;

const GIB: u64 = 1024 * 1024 * 1024;

/// Limite em bytes; zero = desligado. Global ao processo: cada aplicativo
/// tem o seu.
static LIMITE_BYTES: AtomicU64 = AtomicU64::new(LIMITE_PADRAO_GB as u64 * GIB);

/// Valor vindo da configuração: ilegível vira o padrão, zero ou negativo
/// desliga, o resto fica entre o mínimo e o máximo.
pub fn normalizar_gb(valor: f64) -> u16 {
    if !valor.is_finite() {
        return LIMITE_PADRAO_GB;
    }
    let valor = valor.round();
    if valor <= 0.0 { 0 } else { valor.clamp(LIMITE_MIN_GB as f64, LIMITE_MAX_GB as f64) as u16 }
}

/// Define o limite do processo (0 desliga).
pub fn definir_limite_gb(gb: u16) {
    LIMITE_BYTES.store(u64::from(normalizar_gb(f64::from(gb))) * GIB, Ordering::Relaxed);
}

/// Limite atual em bytes (0 = desligado).
pub fn limite_bytes() -> u64 {
    LIMITE_BYTES.load(Ordering::Relaxed)
}

/// Memória física em uso pelo processo agora (RSS / working set).
/// `None` quando o sistema não informa.
pub fn uso_bytes() -> Option<u64> {
    sistema::uso_bytes()
}

/// `Err` com a mensagem para a pessoa quando o processo passou do limite.
/// `contexto` diz o que estava sendo feito ("A leitura do XLSX").
pub fn verificar(contexto: &str) -> Result<(), String> {
    verificar_com(contexto, limite_bytes(), uso_bytes())
}

fn verificar_com(contexto: &str, limite: u64, uso: Option<u64>) -> Result<(), String> {
    let Some(uso) = uso else { return Ok(()) };
    if limite == 0 || uso <= limite {
        return Ok(());
    }
    Err(format!(
        "{contexto} foi interrompida: o programa chegou a {:.1} GB de RAM, acima do limite de {} GB. \
         Ajuste o limite em Configurações (0 desliga).",
        uso as f64 / GIB as f64, limite / GIB,
    ))
}

/// Vigia barata para laços longos: só consulta o sistema a cada `passo`
/// chamadas de [`Vigia::tique`].
pub struct Vigia {
    contexto: &'static str,
    passo: u32,
    contagem: u32,
}

impl Vigia {
    pub fn new(contexto: &'static str, passo: u32) -> Self {
        Self { contexto, passo: passo.max(1), contagem: 0 }
    }

    pub fn tique(&mut self) -> Result<(), String> {
        self.contagem += 1;
        if self.contagem < self.passo {
            return Ok(());
        }
        self.contagem = 0;
        verificar(self.contexto)
    }
}

#[cfg(target_os = "linux")]
mod sistema {
    pub fn uso_bytes() -> Option<u64> {
        std::fs::read_to_string("/proc/self/status").ok()?
            .lines().find(|l| l.starts_with("VmRSS:"))?
            .split_whitespace().nth(1)?
            .parse::<u64>().ok()
            .map(|kib| kib.saturating_mul(1024))
    }
}

#[cfg(windows)]
mod sistema {
    #[repr(C)]
    struct ContadoresMemoria {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> isize;
        fn K32GetProcessMemoryInfo(processo: isize, contadores: *mut ContadoresMemoria, cb: u32) -> i32;
    }

    pub fn uso_bytes() -> Option<u64> {
        let mut c = ContadoresMemoria {
            cb: std::mem::size_of::<ContadoresMemoria>() as u32,
            page_fault_count: 0, peak_working_set_size: 0, working_set_size: 0,
            quota_peak_paged_pool_usage: 0, quota_paged_pool_usage: 0,
            quota_peak_non_paged_pool_usage: 0, quota_non_paged_pool_usage: 0,
            pagefile_usage: 0, peak_pagefile_usage: 0,
        };
        // SAFETY: estrutura do tamanho declarado em `cb`; o pseudo-handle do
        // processo atual não precisa ser fechado.
        let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb) };
        (ok != 0).then_some(c.working_set_size as u64)
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod sistema {
    pub fn uso_bytes() -> Option<u64> { None }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn normaliza_como_a_vigia_antiga_do_feathersheet() {
        assert_eq!(normalizar_gb(f64::NAN), 6);
        assert_eq!(normalizar_gb(0.0), 0);
        assert_eq!(normalizar_gb(-3.0), 0);
        assert_eq!(normalizar_gb(1.0), 2);
        assert_eq!(normalizar_gb(500.0), 128);
        assert_eq!(normalizar_gb(7.4), 7);
    }

    #[test]
    fn so_interrompe_acima_do_limite_e_nunca_desligado() {
        assert!(verificar_com("A leitura", 6 * GIB, Some(5 * GIB)).is_ok());
        assert!(verificar_com("A leitura", 0, Some(500 * GIB)).is_ok());
        assert!(verificar_com("A leitura", 6 * GIB, None).is_ok());
        let erro = verificar_com("A leitura do XLSX", 6 * GIB, Some(7 * GIB)).unwrap_err();
        assert!(erro.starts_with("A leitura do XLSX foi interrompida"), "{erro}");
        assert!(erro.contains("7.0 GB") && erro.contains("6 GB"), "{erro}");
    }

    #[cfg(any(target_os = "linux", windows))]
    #[test]
    fn mede_o_proprio_processo() {
        assert!(uso_bytes().is_some_and(|b| b > 0));
    }
}
