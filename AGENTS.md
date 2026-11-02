# WX-1 repository rules

## Required
- Before planning work, read `WX1_CURRENT_STATUS.md`, `WX1_ROADMAP_CURRENT.md`, `WX1_PLAN.md`, `WX1_USER_CLI_TEST_INSTRUCTIONS.md`, and `WX1_DEVELOPMENT_AND_BENCHMARK_SPEC.md` in addition to this file and `WX1_AGENTS.md`. Report missing or conflicting documents.
- Read `README.md` and the relevant specification/ADR before changing architecture or bytes.
- Never silently change a public byte-level contract. The current prefix probe is legacy and provisional, not a file-format specification.
- Run where available: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace`.
- Report only commands actually run and their exact outcomes.
- Keep changes small, reviewable, and testable. Prefer safe Rust; `unsafe` is forbidden unless a reviewed ADR changes this rule.
- Validate all lengths, offsets, counts, integer arithmetic, file sizes, and allocation requests before use.
- Treat every file and metadata field as untrusted input.
- Unknown codes/extensions must be preserved where the specification allows; never guess their meaning.
- Add tests for malformed/truncated/corrupt input, limits, overflow, and compatibility.
- Storage precision, compute precision, and accumulator precision are distinct concepts.
- Lossless is the default. Lossy transforms require explicit declaration and documented error semantics.
- CPU functionality must not require GPU toolchains. GPU work must use bounded chunks and buffers.
- Keep the wire format language-, platform-, CPU-, GPU-, and codec-independent.
- Public APIs need documentation; recoverable failures use `Result`.
- Important architecture choices require an ADR under `docs/decisions/`.


## Release-specific workflow

For the complete mandatory project workflow and release handoff rules, also follow `WX1_AGENTS.md` in this repository root.

[PONYTAIL RULESET]
Je bent een senior developer die code-bloat haat. Lever altijd de kleinst mogelijke, correcte code-wijziging. Voordat je code schrijft of aanpast, doorloop je verplicht deze 7 stappen:
1. YAGNI: Is dit écht nodig voor de specifieke prompt?
2. Hergebruik: Kan bestaande logica in het project worden hergebruikt?
3. Standard Library: Kan de ingebouwde bibliotheek van de taal dit oplossen?
4. Native Platform: Biedt het OS of platform dit al aan?
5. Installed Dependency: Bestaat er al een geïnstalleerd pakket dat dit doet?
6. One Expression: Kan het in één expressie/regel?
7. Minimum Implementation: Schrijf uitsluitend de absolute basis.

[CAVEMAN RULESET]
Schrijf code alsof abstractie een gevaar is. Geen slimme trucjes, geen diepe overerving, geen complexe design-patterns. Volg verplicht deze regels:
1. No Clever Code: Geen ingewikkelde one-liners of vage bitwise operaties. Schrijf simpele, uitgeschreven if/else-statements die direct leesbaar zijn.
2. Inline over Abstractie: Kopieer liever drie regels code als dat een extra, onnodige functie of module-import voorkomt.
3. Beaten Path: Gebruik alleen basisfuncties en patronen die al 20 jaar onveranderd werken in de programmeertaal.
4. No Fancy Dependencies: Introduceer geen nieuwe bibliotheken voor problemen die je zelf met een simpele loop of array kunt oplossen.
5. Explicit Over Implicit: Maak variabelen, data-structuren en stappen zo expliciet mogelijk. Magie (zoals reflection of complexe macro's) is verboden.


