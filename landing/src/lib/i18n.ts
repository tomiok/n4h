import { benchmarkCopy } from "./benchmark-copy";
import { referenceCopy } from "./reference-copy";

export const locales = ["en", "es"] as const;
export type Locale = (typeof locales)[number];

export function isLocale(value: string): value is Locale {
  return locales.some((locale) => locale === value);
}

const en = {
  metadata: {
    description:
      "Technical overview of n4h: persistent search references, verifiable text edits, optional indexing, Rust AST rewrites, token budgets, and explicit agent memory. Initial Rust MVP verified on Linux.",
    ogLocale: "en_US",
  },
  navigation: {
    skip: "Skip to content",
    home: "n4h, home",
    label: "Main navigation",
    workflow: "How it works",
    documentation: "Docs",
    languages: "Language",
  },
  hero: {
    tagline: "A CLI for coding agents",
    title: "Not for human.",
    subtitle: "Built for agents.",
    description: "Persistent search and verifiable edits for coding agents.",
    scope:
      "Initial MVP · Verified on Linux · Multi-file edits are recoverable, not atomic.",
    start: "Read the docs",
    source: "View the code",
    bottom: "Technical overview based on the repository README",
    explore: "Command workflow",
    installTitle: "Install n4h",
    installNote:
      "Linux x86-64 · GNU libc ≥ 2.35 · Review the script before running it. The installer asks for confirmation.",
  },
  principles: [
    "Persistent references",
    "Immutable plans",
    "JSON output",
    "Recoverable checkpoints",
  ],
  workflow: {
    eyebrow: "01 — THE WORKFLOW",
    title: ["Search, inspect, plan.", "Apply or undo."],
    description:
      "find persists a result snapshot. edit captures immutable before/after bytes without changing sources. apply verifies the targets before writing; undo restores a checkpoint after checking the current state.",
  },
  demo: {
    label: "Workflow steps",
    terminal: "n4h / workflow",
    note: "Illustrative example · use the identifiers returned by the CLI.",
    steps: [
      {
        title: "find → inspect",
        description:
          "Results are saved before the first page is returned. Inspect matches and follow cursors without rerunning the search. Results are snapshots, not live views.",
        comments: [
          "# Keep the result ID returned by n4h.",
          "# Inspect the context around the match.",
        ],
      },
      {
        title: "edit → inspect",
        description:
          "Verify selected file hashes and capture an immutable plan. No source is changed. Read every diff page before applying; --scope all selects all saved matches, not just the first page.",
        comments: ["# Creates a plan. No source files are changed yet."],
      },
      {
        title: "apply",
        description:
          "Preflight checks all targets before the first write. A preflight conflict changes no sources. Files are then replaced sequentially; an interrupted operation may be partially applied and require recovery.",
        comments: [
          "# Verify hashes and save a checkpoint.",
          "# Apply the bytes captured in the plan.",
          "# Multi-file operations are recoverable, not atomic.",
        ],
      },
      {
        title: "undo",
        description:
          "Verify every post-operation hash and permission before restoring the whole checkpoint. A conflict refuses the restore. Completed undo replays return a historical receipt, not a check of current source bytes.",
        comments: [
          "# Check hashes before restoring.",
          "# Return a conflict if external changes are detected.",
          "# Restore the whole checkpoint as a recoverable operation.",
        ],
      },
    ],
  },
  features: {
    eyebrow: "02 — DOCUMENTED CAPABILITIES",
    title: ["Search modes, output", "and session state."],
    source: "README section",
    items: [
      {
        title: "Literal and regex search",
        description:
          "Literal by default; --regex uses Rust regex syntax. Matches are line-based. Ignore rules are respected; hidden paths and symlinks are excluded. Results and unapplied plans expire after 24 hours.",
        label: "find / inspect",
      },
      {
        title: "Optional trigram index",
        description:
          "--indexed queries a cached, case-sensitive literal snapshot. Changes require index update; --verify rereads admitted files. No indexed regex or AST, automatic fallback, or live freshness guarantee.",
        label: "index build / update / clear",
      },
      {
        title: "Rust AST search and rewrites",
        description:
          "ast-grep and tree-sitter match Rust syntax and expand captured nodes. Rewritten outputs are syntax-checked before saving a plan. This does not resolve symbols or types, run rustfmt, or guarantee compilation.",
        label: "ast find / ast edit",
      },
      {
        title: "Exact output token budgets",
        description:
          "Pair --max-tokens with --tokenizer to bound the complete JSON response, including its final newline. Supports o200k_base and cl100k_base; smaller pages don’t reduce the edit scope.",
        label: "--max-tokens / --tokenizer",
      },
      {
        title: "Explicit agent memory",
        description:
          "Store repository-local notes with mem put and retrieve them with mem get. Revision checks protect updates. No automatic conversation capture or semantic memory.",
        label: "mem put / get / list / search",
      },
      {
        title: "Read-only session summary",
        description:
          "Use resume to inspect pending recovery, recent records, and agent notes between sessions. A read-only summary—not a model call or automatic task continuation.",
        label: "resume / resume --limit",
      },
    ],
    scope:
      "Rust MVP, verified on Linux. Text edits, not semantic refactoring. Multi-file operations are recoverable, not atomic.",
  },
  docs: {
    eyebrow: "03 — DOCUMENTATION",
    title: ["Installation", "and command examples."],
    description:
      "Examples adapted from the README. Commands run locally with embedded redb storage; no database server or external rg binary is needed. The web examples do not execute n4h.",
    requirements:
      "Release target: Linux x86-64, GNU libc ≥ 2.35 · Bash, curl, tar, sha256sum · Rust + C compiler only for source builds",
    steps: {
      install: {
        title: "Install the published binary",
        description:
          "Downloads the latest published Linux x86-64 release, verifies its SHA-256 checksum, and offers Claude Code, Codex, OpenCode, or CLI-only installation without sudo. Rust and Git are not required for the prebuilt binary.",
        note: "Review downloaded scripts before executing them. Choose one or more harnesses and user or current-project scope. No shell profiles, permissions, project initialization, or indexing are changed automatically.",
      },
      source: {
        title: "Source fallback for developers",
        description:
          "Alternatively, clone the repository and compile the normal binary with stable Rust and a C compiler. The source build also includes setup and doctor.",
        note: "Do not install with --all-features: it enables test-only fault injection. Cargo installs into its configured bin directory; make sure your harness can find n4h on PATH.",
      },
      setup: {
        title: "Connect your harness and verify",
        description:
          "The installer calls setup unless you choose CLI-only. Run it later to add harnesses; select user-wide or project-local skills. Use --dry-run to preview explicit destinations and doctor to inspect PATH and skill files without initializing a project.",
        note: "Skills teach the agent to call n4h through its terminal; this is not MCP and does not force usage. Different existing skills are never overwritten. OpenCode may also discover Claude/Codex compatibility copies; native/project skills can take precedence.",
      },
      search: {
        title: "Initialize your repository and search",
        description:
          "Replace /path/to/your/project with your repository root. init creates local state; find saves a searchable result snapshot.",
        note: "Normal commands return bounded JSON on stdout. Capture the result and match IDs from the actual response.",
      },
      edit: {
        title: "Inspect, plan, apply, undo",
        description:
          "Replace r_RESULT, m_MATCH, p_PLAN, and cp_CHECKPOINT with the real IDs returned by each command. These are placeholders, not executable IDs.",
        note: "Read every diff page using cursor_next before applying. edit only prepares a plan; apply writes files. Run your project’s checks afterward.",
      },
      tokens: {
        title: "Bound the output in tokens",
        description:
          "Use --max-tokens and --tokenizer together. Choose o200k_base or cl100k_base and a limit from 256 to 1,000,000. Replace the result and cursor placeholders with real response IDs.",
        note: "The count covers serialized JSON plus its final newline, not model billing or input tokens. The 64 KiB limit still applies. If a complete unit cannot fit, n4h returns output_budget_too_small rather than cutting JSON or changing the edit selection.",
      },
      memory: {
        title: "Save notes and resume context",
        description:
          "Write an explicit project note, retrieve it, then ask for a bounded session summary. resume separates historical records from agent-authored notes and prioritizes pending recovery.",
        note: "mem put creates only unless you supply --if-revision for an update. Notes are not verified facts. resume is read-only: it does not run the saved test command, apply a plan, or continue a task automatically.",
      },
      index: {
        title: "Build and update the optional index",
        description:
          "Build a repository snapshot before using --indexed. update reevaluates paths and ignore rules; --verify rereads and hashes all admitted source files.",
        note: "Only case-sensitive literal search is indexed. The cache may be stale until updated and stores complete admitted source contents. index clear removes the cache, not notes, plans, or checkpoints.",
      },
      ast: {
        title: "Search and rewrite Rust syntax",
        description:
          "Quote $ variables so the shell does not expand them. Replace result and match placeholders with actual IDs. AST results require ast edit, not ordinary edit.",
        note: "Only the Rust grammar is enabled. The rewrite checks syntax, not symbol resolution, types, compilation, or behavior. Run cargo check and your tests yourself after applying.",
      },
    },
    clipboard: {
      label: "Copy commands",
      copy: "Copy ⧉",
      copied: "Copied ✓",
      error: "Couldn’t copy. Select the commands and copy them manually.",
    },
    guidesTitle: "README references",
    guides: {
      tokens: {
        title: "Exact output token budgets",
        description:
          "Paired flags, tokenizers, bounded pages, and explicit refusals.",
      },
      index: {
        title: "Search & indexing",
        description: "Optional snapshots and explicit index updates.",
      },
      ast: {
        title: "Rust AST rewrites",
        description: "Structural matching, captures, and syntax checks.",
      },
      memory: {
        title: "Memory & resume",
        description: "Explicit notes and read-only resume summaries.",
      },
      safety: {
        title: "Conflicts & recovery",
        description:
          "Preflight checks, checkpoints, and interrupted operations.",
      },
    },
    reference: "Full CLI reference on GitHub",
    contractTitle: "MVP contract",
    limits: [
      "Up to 50 files per plan",
      "2 MiB maximum per source file",
      "64 KiB maximum per JSON response",
    ],
    safety:
      "Multi-file writes are recoverable, not atomic. Work without concurrent writers, preferably in a dedicated worktree.",
    state:
      "State includes source snapshots and notes; optional indexing stores complete admitted files. It is not encrypted. Add .n4h/ to your ignore policy yourself.",
  },
  technical: referenceCopy.en,
  benchmarks: {
    eyebrow: "06 — REPRODUCIBILITY",
    title: ["Benchmarks", "and reproducibility."],
    description:
      "A team-reported task comparison with its outcomes and limitations. Repository demos and the synthetic index benchmark are documented separately below.",
    taskComparison: benchmarkCopy.en,
    caption: "Synthetic index benchmark results",
    columns: ["Test", "n4h", "Baseline", "Context"],
    methodology: "View methodology and environment",
  },
  footer: {
    tagline: "Not for Human. Built for agents.",
    documentation: "Documentation",
    bottom:
      "Repository documentation + team-reported CLI benchmarks · Initial Linux MVP",
  },
};

export type Dictionary = typeof en;

const es: Dictionary = {
  metadata: {
    description:
      "Resumen técnico de n4h: búsquedas persistentes, ediciones verificables, índice opcional, reescrituras AST de Rust, límites de tokens y memoria explícita. MVP inicial en Rust verificado en Linux.",
    ogLocale: "es_ES",
  },
  navigation: {
    skip: "Saltar al contenido",
    home: "n4h, inicio",
    label: "Navegación principal",
    workflow: "Cómo funciona",
    documentation: "Docs",
    languages: "Idioma",
  },
  hero: {
    tagline: "Una CLI para agentes de código",
    title: "Not for human.",
    subtitle: "Built for agents.",
    description:
      "Búsquedas persistentes y ediciones verificables para agentes de código.",
    scope:
      "MVP inicial · Verificado en Linux · Ediciones multiarchivo recuperables, no atómicas.",
    start: "Ver documentación",
    source: "Ver el código",
    bottom: "Resumen técnico basado en el README del repositorio",
    explore: "Flujo de comandos",
    installTitle: "Instalar n4h",
    installNote:
      "Linux x86-64 · GNU libc ≥ 2.35 · Revisa el script antes de ejecutarlo. El instalador pide confirmación.",
  },
  principles: [
    "Referencias persistentes",
    "Planes inmutables",
    "Salida JSON",
    "Checkpoints recuperables",
  ],
  workflow: {
    eyebrow: "01 — EL FLUJO",
    title: ["Busca, inspecciona, planifica.", "Aplica o deshaz."],
    description:
      "find persiste un snapshot de resultados. edit captura los bytes de antes y después sin modificar el código. apply verifica los archivos antes de escribir; undo restaura un checkpoint tras comprobar el estado actual.",
  },
  demo: {
    label: "Pasos del flujo",
    terminal: "n4h / flujo",
    note: "Ejemplo ilustrativo · usa los identificadores devueltos por la CLI.",
    steps: [
      {
        title: "find → inspect",
        description:
          "Los resultados se guardan antes de devolver la primera página. Inspecciona coincidencias y sigue los cursores sin repetir la búsqueda. Son snapshots, no vistas en vivo.",
        comments: [
          "# Conserva el ID de resultado devuelto por n4h.",
          "# Inspecciona el contexto de la coincidencia.",
        ],
      },
      {
        title: "edit → inspect",
        description:
          "Verifica los hashes seleccionados y captura un plan inmutable sin modificar archivos. Lee todas las páginas del diff antes de aplicar; --scope all selecciona todas las coincidencias guardadas, no solo la primera página.",
        comments: ["# Crea un plan. Todavía no modifica el código."],
      },
      {
        title: "apply",
        description:
          "Comprueba todos los archivos antes de la primera escritura. Un conflicto previo no cambia el código. Después reemplaza archivos secuencialmente; una interrupción puede dejar una operación parcial que requiere recuperación.",
        comments: [
          "# Verifica hashes y guarda un checkpoint.",
          "# Aplica los bytes capturados en el plan.",
          "# Las operaciones multiarchivo son recuperables, no atómicas.",
        ],
      },
      {
        title: "undo",
        description:
          "Comprueba todos los hashes y permisos posteriores antes de restaurar el checkpoint completo. Si hay conflicto, rechaza la restauración. Repetir un undo completado devuelve un recibo histórico, no verifica el código actual.",
        comments: [
          "# Comprueba los hashes antes de restaurar.",
          "# Si detecta cambios externos, devuelve un conflicto.",
          "# Restaura el checkpoint completo como operación recuperable.",
        ],
      },
    ],
  },
  features: {
    eyebrow: "02 — CAPACIDADES DOCUMENTADAS",
    title: ["Modos de búsqueda, salida", "y estado de sesión."],
    source: "Sección del README",
    items: [
      {
        title: "Búsqueda literal y regex",
        description:
          "Literal por defecto; --regex usa la sintaxis regex de Rust. Las coincidencias se evalúan por línea. Respeta las reglas de exclusión y omite rutas ocultas y enlaces simbólicos. Resultados y planes sin aplicar caducan a las 24 horas.",
        label: "find / inspect",
      },
      {
        title: "Índice opcional de trigramas",
        description:
          "--indexed consulta un snapshot literal sensible a mayúsculas. Los cambios requieren index update; --verify relee los archivos admitidos. Sin regex o AST indexados, fallback automático ni garantía de actualidad.",
        label: "index build / update / clear",
      },
      {
        title: "Búsqueda y reescritura AST de Rust",
        description:
          "ast-grep y tree-sitter buscan sintaxis Rust y expanden nodos capturados. Valida la sintaxis del resultado antes de guardar el plan. No resuelve símbolos o tipos ni ejecuta rustfmt; tampoco garantiza la compilación.",
        label: "ast find / ast edit",
      },
      {
        title: "Límites exactos de tokens de salida",
        description:
          "Combina --max-tokens con --tokenizer para acotar el JSON completo, incluido el salto de línea final. Admite o200k_base y cl100k_base; reducir la página no reduce el alcance de la edición.",
        label: "--max-tokens / --tokenizer",
      },
      {
        title: "Memoria explícita del agente",
        description:
          "Guarda notas locales con mem put y recupéralas con mem get. Las revisiones protegen las actualizaciones. Sin capturar conversaciones automáticamente ni memoria semántica.",
        label: "mem put / get / list / search",
      },
      {
        title: "Resumen de sesión de solo lectura",
        description:
          "Consulta con resume la recuperación pendiente, los registros recientes y las notas entre sesiones. Es un resumen de solo lectura: no llama al modelo ni continúa tareas automáticamente.",
        label: "resume / resume --limit",
      },
    ],
    scope:
      "MVP en Rust, verificado en Linux. Ediciones de texto, no refactorización semántica. Las operaciones multiarchivo son recuperables, no atómicas.",
  },
  docs: {
    eyebrow: "03 — DOCUMENTACIÓN",
    title: ["Instalación", "y ejemplos de comandos."],
    description:
      "Ejemplos adaptados del README. Los comandos se ejecutan localmente con redb embebido, sin servidor de base de datos ni binario externo de rg. Los ejemplos de la web no ejecutan n4h.",
    requirements:
      "Release: Linux x86-64, GNU libc ≥ 2.35 · Bash, curl, tar, sha256sum · Rust + compilador C solo para compilar desde fuente",
    steps: {
      install: {
        title: "Instala el binario publicado",
        description:
          "Descarga la última release publicada para Linux x86-64, verifica su checksum SHA-256 y ofrece Claude Code, Codex, OpenCode o solo CLI, sin sudo. El binario precompilado no requiere Rust ni Git.",
        note: "Revisa los scripts descargados antes de ejecutarlos. Elige uno o varios harnesses y alcance de usuario o del proyecto actual. No cambia perfiles de shell ni permisos, ni inicializa o indexa proyectos automáticamente.",
      },
      source: {
        title: "Alternativa desde fuente para desarrolladores",
        description:
          "Como alternativa, clona el repositorio y compila el binario normal con Rust estable y un compilador C. La compilación desde fuente también incluye setup y doctor.",
        note: "No instales con --all-features: activa la inyección de fallos reservada a pruebas. Cargo instala en su directorio bin configurado; comprueba que tu harness encuentre n4h en PATH.",
      },
      setup: {
        title: "Conecta tu harness y verifica",
        description:
          "El instalador llama a setup salvo que elijas solo CLI. Ejecútalo después para añadir harnesses; selecciona skills para tu usuario o para un proyecto. Usa --dry-run para previsualizar destinos explícitos y doctor para revisar PATH y skills sin inicializar un proyecto.",
        note: "Las skills enseñan al agente a llamar a n4h desde su terminal; no es MCP ni fuerza su uso. Nunca sobrescribe skills distintas existentes. OpenCode también puede descubrir copias compatibles de Claude/Codex; las skills nativas o del proyecto pueden tener prioridad.",
      },
      search: {
        title: "Inicializa tu repositorio y busca",
        description:
          "Sustituye /path/to/your/project por la raíz de tu repositorio. init crea el estado local; find guarda un snapshot de resultados.",
        note: "Los comandos normales devuelven JSON acotado en stdout. Guarda los IDs de resultado y coincidencia de la respuesta real.",
      },
      edit: {
        title: "Inspecciona, planifica, aplica y deshaz",
        description:
          "Sustituye r_RESULT, m_MATCH, p_PLAN y cp_CHECKPOINT por los IDs reales devueltos por cada comando. Son marcadores de ejemplo, no IDs ejecutables.",
        note: "Lee todas las páginas del diff con cursor_next antes de aplicar. edit solo prepara un plan; apply escribe archivos. Ejecuta después las comprobaciones de tu proyecto.",
      },
      tokens: {
        title: "Limita la salida en tokens",
        description:
          "Usa --max-tokens y --tokenizer juntos. Elige o200k_base o cl100k_base y un límite entre 256 y 1.000.000. Sustituye los marcadores de resultado y cursor por los IDs reales de la respuesta.",
        note: "El recuento incluye el JSON serializado y su salto de línea final, no la facturación del modelo ni los tokens de entrada. Sigue vigente el límite de 64 KiB. Si no cabe una unidad completa, n4h devuelve output_budget_too_small en lugar de cortar el JSON o cambiar la selección de edición.",
      },
      memory: {
        title: "Guarda notas y retoma el contexto",
        description:
          "Escribe una nota explícita del proyecto, recupérala y consulta un resumen acotado de sesión. resume separa los registros históricos de las notas del agente y prioriza la recuperación pendiente.",
        note: "mem put solo crea notas salvo que pases --if-revision para actualizar. Las notas no son hechos verificados. resume es de solo lectura: no ejecuta el comando de pruebas guardado, no aplica planes ni continúa tareas automáticamente.",
      },
      index: {
        title: "Construye y actualiza el índice opcional",
        description:
          "Construye un snapshot del repositorio antes de usar --indexed. update reevalúa las rutas y exclusiones; --verify relee y calcula hashes de todos los archivos admitidos.",
        note: "Solo se indexan búsquedas literales sensibles a mayúsculas. La caché puede quedar desactualizada y guarda el contenido completo de los archivos admitidos. index clear elimina la caché, no notas, planes ni checkpoints.",
      },
      ast: {
        title: "Busca y reescribe sintaxis Rust",
        description:
          "Entrecomilla las variables $ para evitar que el shell las expanda. Sustituye los marcadores de resultado y coincidencia por IDs reales. Los resultados AST requieren ast edit, no edit normal.",
        note: "Solo está habilitada la gramática de Rust. La reescritura comprueba sintaxis, no símbolos, tipos, compilación ni comportamiento. Ejecuta cargo check y tus pruebas después de aplicar.",
      },
    },
    clipboard: {
      label: "Copiar comandos",
      copy: "Copiar ⧉",
      copied: "Copiado ✓",
      error:
        "No se pudo copiar. Selecciona los comandos y cópialos manualmente.",
    },
    guidesTitle: "Referencias del README",
    guides: {
      tokens: {
        title: "Límites exactos de tokens de salida",
        description:
          "Flags, tokenizadores, páginas acotadas y rechazos explícitos.",
      },
      index: {
        title: "Búsqueda e indexación",
        description:
          "Snapshots opcionales y actualización explícita del índice.",
      },
      ast: {
        title: "Reescrituras AST en Rust",
        description: "Búsqueda estructural, capturas y validación sintáctica.",
      },
      memory: {
        title: "Memoria y resume",
        description: "Notas explícitas y resúmenes de solo lectura con resume.",
      },
      safety: {
        title: "Conflictos y recuperación",
        description:
          "Comprobaciones previas, checkpoints y operaciones interrumpidas.",
      },
    },
    reference: "Referencia completa en GitHub (inglés)",
    contractTitle: "Contrato del MVP",
    limits: [
      "Hasta 50 archivos por plan",
      "Máximo de 2 MiB por archivo de origen",
      "Máximo de 64 KiB por respuesta JSON",
    ],
    safety:
      "Las escrituras multiarchivo son recuperables, no atómicas. Trabaja sin escritores concurrentes, preferiblemente en un worktree dedicado.",
    state:
      "El estado incluye snapshots de código y notas; el índice opcional guarda archivos completos. No está cifrado. Añade .n4h/ a tu política de exclusión de Git.",
  },
  technical: referenceCopy.es,
  benchmarks: {
    eyebrow: "06 — REPRODUCIBILIDAD",
    title: ["Benchmarks", "y reproducibilidad."],
    description:
      "Comparativa de tareas reportada por el equipo, con resultados y limitaciones. Más abajo se documentan por separado las demos y el benchmark sintético del índice.",
    taskComparison: benchmarkCopy.es,
    caption: "Resultados del benchmark sintético del índice",
    columns: ["Prueba", "n4h", "Referencia", "Contexto"],
    methodology: "Ver metodología y entorno",
  },
  footer: {
    tagline: "Not for Human. Hecho para agentes.",
    documentation: "Documentación",
    bottom:
      "Documentación del repositorio + benchmarks CLI reportados por el equipo · MVP inicial en Linux",
  },
};

export const dictionaries: Record<Locale, Dictionary> = { en, es };
