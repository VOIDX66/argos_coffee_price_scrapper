# Reglas de Arquitectura y Anti-Slop — Microservicio `argos_coffee_price_scrapper`

> **DIRECTIVA PARA AGENTES DE IA:**
> Este repositorio es un microservicio satélite del ecosistema **Argos** (`crates/argos_coffee_price_scrapper`). Cualquier modificación debe cumplir estrictamente con Clean Architecture, principios SOLID y el contrato de protocolo **TOON** de Argos.

---

## 1. Estructura de Capas (Clean Architecture)

```text
src/
├── domain/                          # [DOMINIO PURO - Cero dependencias de HTTP/Redis/HTML]
│   ├── entities/
│   │   └── coffee_market_indicators.rs  # Entidad + serialización .to_toon() y JSON
│   ├── errors.rs                        # DomainError tipado (sin Box<dyn Error>)
│   └── repositories.rs                  # Traits CoffeeMarketProvider y CoffeeIndicatorsCache
├── application/                     # [CASOS DE USO]
│   └── use_cases/
│       └── get_coffee_market_indicators.rs # Cache-Aside + coalescencia con tokio::sync::Mutex
├── infrastructure/                  # [ADAPTADORES EXTERNOS]
│   ├── providers/
│   │   └── scraper_market_provider.rs   # Scraper FNC con reqwest::Client compartido y LazyLock<Selector>
│   └── repositories/
│       ├── memory_cache.rs              # Caché en memoria con TTL (por defecto, 0 dependencias externas)
│       └── redis_cache.rs               # Caché Redis opcional (cuando se define REDIS_URL)
├── presentation/                    # [TRANSPORTE HTTP AXUM]
│   └── http.rs                          # Endpoints: /health, /info (TOON), /coffee (TOON), /price (TOON), /json
├── lib.rs
└── main.rs                          # Inyección de dependencias y spawn_cache_warmup al arranque
```

---

## 2. Prohibiciones Estrictas (Cero AI Slop)

1. **Prohibido `.unwrap()` o `.expect()` fuera de `#[cfg(test)]`:**
   - Todos los errores de red, parseo HTML o caché deben convertirse en variantes de `DomainError`.
2. **Selectores CSS precompilados (`LazyLock`):**
   - Nunca parses selectores `scraper::Selector::parse` en cada petición; usa estáticos `LazyLock<Selector>`.
3. **Coalescencia y Precalentamiento:**
   - Mantén el `tokio::sync::Mutex` en `GetCoffeeMarketIndicatorsUseCase` para evitar múltiples scrapings simultáneos ante ráfagas de peticiones.
   - Mantén `spawn_cache_warmup` en `main.rs` para que la primera consulta desde Argos responda en milisegundos.
4. **Contrato HTTP de Argos (Puerto `8084` por defecto):**
   - `GET /health` → `"ok"`
   - `GET /info` → Descriptor en formato **TOON**
   - `GET /coffee` y `GET /price` → Indicadores en formato **TOON** (`text/plain; charset=utf-8`)
   - `GET /json` → Indicadores en formato JSON (`application/json`)
5. **Verificación obligatoria antes de commit:**
   - Ejecutar `cargo test` y asegurar que todos los tests de dominio, casos de uso, scraper y endpoints HTTP pasen al 100%.
