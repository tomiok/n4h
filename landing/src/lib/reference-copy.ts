// Technical summaries and translations of the repository README.
// Keep the qualifications alongside each capability; do not turn them into guarantees.
const en = {
  source: "Source: repository README",
  contentsLabel: "On this page",
  contents: [
    "Workflow",
    "Capabilities",
    "Examples",
    "CLI contract",
    "Safety & state",
    "Reproducibility",
  ],
  contract: {
    eyebrow: "04 — CLI CONTRACT",
    title: ["JSON, pagination", "and resource limits."],
    description:
      "Normal commands emit one bounded JSON object on stdout. --help and --version emit plain text. --json is accepted explicitly for agent compatibility.",
    protocols: [
      {
        title: "Response fields",
        description:
          "Success includes schema_version, ok: true, and kind. Errors include error.code, message, paths, next_actions, and an operation ID when recovery is relevant. retryable: false means mutations should not be blindly repeated.",
      },
      {
        title: "Independent cursors",
        description:
          "Follow cursor_next for result/diff pages and files_cursor_next for plan file metadata. Cursors are opaque and scoped to their saved object. A smaller page never narrows the selected edit scope.",
      },
      {
        title: "Token-bounded output",
        description:
          "--max-tokens requires --tokenizer. Counts include the complete compact JSON and final newline. Token-bounded diff chunks end at a line boundary or EOF; concatenate by diff_start_byte. An indivisible unit that cannot fit returns output_budget_too_small.",
      },
      {
        title: "Selected encoding, not provider usage",
        description:
          "o200k_base and cl100k_base are bundled locally. The tokenizer is not inferred from a model. Input tokens, chat/tool framing, provider billing, and total task savings are not measured. Tokenization adds CPU/startup cost and small pages repeat metadata.",
      },
    ],
    exitTitle: "Exit codes",
    exitCaption: "Documented CLI exit codes",
    exitHeaders: ["Exit", "Meaning"],
    exits: [
      {
        code: "0",
        meaning: "Success, including zero matches and idempotent replay",
      },
      {
        code: "2",
        meaning: "Invalid CLI or selection; missing initialization",
      },
      {
        code: "3",
        meaning:
          "Conflict, expiry, missing reference, no changes, busy, or recovery required",
      },
      { code: "4", meaning: "I/O, storage, or stored-data integrity failure" },
      { code: "5", meaning: "Resource limit or unsupported capability/file" },
    ],
    limitsTitle: "Selected MVP limits",
    limitsCaption:
      "Resource bounds from Limits and state in the README; not performance measurements",
    limitHeaders: ["Resource", "Limit"],
    limits: [
      { resource: "Source file", limit: "2 MiB" },
      {
        resource: "Files / source bytes per search",
        limit: "20,000 / 100 MiB",
      },
      { resource: "Matches per result", limit: "20,000" },
      { resource: "Files per plan", limit: "50" },
      { resource: "Combined before/after bytes per plan", limit: "20 MiB" },
      { resource: "JSON output, including newline", limit: "64 KiB" },
      { resource: "Optional token budget", limit: "256–1,000,000" },
      { resource: "Result / unapplied plan TTL", limit: "24 hours" },
      { resource: "Durable repository state quota", limit: "200 MiB" },
      { resource: "Live notes / text per note", limit: "1,024 / 8 KiB" },
      { resource: "Combined live note keys and text", limit: "4 MiB" },
      { resource: "Default separate index cache", limit: "512 MiB" },
    ],
  },
  recovery: {
    eyebrow: "05 — SAFETY AND STATE",
    title: ["Conflict checks", "and explicit recovery."],
    description:
      "redb transactions protect internal state, not an atomic transaction over external files. The following qualifications are part of the documented contract.",
    items: [
      {
        title: "Before the first write",
        description:
          "apply checks all target hashes and basic permissions. It durably stores snapshots, a checkpoint, and a per-file journal before replacing sources. A preflight conflict changes no sources.",
      },
      {
        title: "Sequential file replacement",
        description:
          "An intermediate failure can leave some files changed. There is no automatic rollback. Pending recovery blocks new plans, applies, and undos; inspection and diagnostic commands remain available.",
      },
      {
        title: "Undo and replay",
        description:
          "undo preflights every post-operation hash and permission before restoring the complete checkpoint. Replays return historical receipts, not proof of current source state. Committed operations use undo, not recovery rollback.",
      },
      {
        title: "Concurrent writers",
        description:
          "The repository lock coordinates n4h processes, not editors or other writers. A race remains between the final hash check and rename. Use a dedicated worktree without concurrent writers; this is not a sandbox for hostile trees.",
      },
      {
        title: "Persisted source data",
        description:
          "State is stored in .n4h/state.redb; optional full-source index content is in .n4h/index.redb. Notes, search previews, before/after bytes, and checkpoints can contain secrets. Storage is not encrypted and has no secure-erasure or --no-store guarantee. Exclude .n4h/ from Git yourself.",
      },
      {
        title: "Expiry and compatibility",
        description:
          "TTL invalidates references; it does not erase rows or checkpoints. There is no automatic garbage collection. Legacy SQLite state is rejected with legacy_state_detected, not silently imported. macOS, Windows, network filesystems, and power-loss durability are not certified.",
      },
    ],
    inspectTitle: "Inspect pending operations",
    inspectNote:
      "Without --rollback, recover is inspection-only. Replace op_OPERATION with the actual ID and follow any returned cursor before deciding what to do.",
    rollbackTitle: "Explicit rollback of a pending operation",
    rollbackNote:
      "This command restores source bytes. Rollback accepts only the expected before/after versions and permissions. Unexpected edits return recovery_conflict. There is no force-overwrite; preserve .n4h/ and reconcile external changes instead of deleting checkpoints.",
    scopeTitle: "Not implemented in this MVP",
    scopeDescription:
      "These are current scope exclusions, not release commitments.",
    exclusions: [
      "Symbol-aware rename, LSP/type-aware refactoring, or AST languages other than Rust.",
      "Indexed regex/AST queries, filesystem watcher/daemon, or automatic cache compaction.",
      "Embeddings, semantic memory, automatic conversation capture, or MCP.",
      "Custom tokenizers, automatic model/tokenizer detection, provider billing estimates, or an agent-model benchmark.",
      "Operational-state retention/export/cleanup commands or production cross-platform support.",
    ],
  },
  reproducibility: {
    demosTitle: "Isolated demo drivers",
    demosDescription:
      "Python 3 is needed for these drivers, not for n4h itself. They create their own fixtures and capture actual IDs. The optional AST --check-rust mode additionally needs rustc and checks only the demo fixture.",
    checksTitle: "Build and checks",
    checksDescription:
      "These commands are documented under Build. --all-features is for testing; do not use it for cargo install because it enables fault injection.",
    indexTitle: "Synthetic index benchmark",
    indexDescription:
      "Uses an isolated fixture, checks live/indexed match equality, alternates query order, and records each sample plus median/range. It also checks no-op, one-file, and verified index updates.",
    limitsTitle: "Measurement scope",
    limitations: [
      "Timing includes process startup, JSON output, and result persistence; OS caches are not cleared.",
      "The corpus is repetitive synthetic text with a selective query. Results are exploratory, not a general speedup guarantee.",
      "Peak memory, model tokens, provider usage, and total agent-task cost are not measured.",
    ],
  },
};

const es: typeof en = {
  source: "Fuente: README del repositorio",
  contentsLabel: "En esta página",
  contents: [
    "Flujo",
    "Capacidades",
    "Ejemplos",
    "Contrato CLI",
    "Seguridad y estado",
    "Reproducibilidad",
  ],
  contract: {
    eyebrow: "04 — CONTRATO DE LA CLI",
    title: ["JSON, paginación", "y límites de recursos."],
    description:
      "Los comandos normales emiten un objeto JSON acotado en stdout. --help y --version emiten texto plano. --json se acepta explícitamente para compatibilidad con agentes.",
    protocols: [
      {
        title: "Campos de respuesta",
        description:
          "El éxito incluye schema_version, ok: true y kind. Los errores incluyen error.code, message, paths, next_actions y un ID de operación cuando hace falta recuperar. retryable: false indica que no se deben repetir mutaciones a ciegas.",
      },
      {
        title: "Cursores independientes",
        description:
          "Sigue cursor_next para páginas de resultados/diff y files_cursor_next para metadatos de archivos del plan. Los cursores son opacos y pertenecen al objeto guardado. Una página menor no reduce la selección de edición.",
      },
      {
        title: "Salida acotada en tokens",
        description:
          "--max-tokens requiere --tokenizer. El recuento incluye el JSON compacto completo y el salto de línea final. Los fragmentos de diff acotados en tokens terminan en fin de línea o EOF; se concatenan mediante diff_start_byte. Una unidad indivisible que no cabe produce output_budget_too_small.",
      },
      {
        title: "Codificación elegida, no consumo del proveedor",
        description:
          "o200k_base y cl100k_base se incluyen localmente. El tokenizador no se deduce del modelo. No se miden tokens de entrada, formato de chat/herramientas, facturación ni ahorro total de la tarea. Tokenizar añade coste de CPU y arranque; las páginas pequeñas repiten metadatos.",
      },
    ],
    exitTitle: "Códigos de salida",
    exitCaption: "Códigos de salida documentados de la CLI",
    exitHeaders: ["Código", "Significado"],
    exits: [
      {
        code: "0",
        meaning: "Éxito, incluidas cero coincidencias y repetición idempotente",
      },
      { code: "2", meaning: "CLI o selección no válida; falta inicialización" },
      {
        code: "3",
        meaning:
          "Conflicto, caducidad, referencia ausente, sin cambios, ocupado o recuperación requerida",
      },
      {
        code: "4",
        meaning:
          "Error de E/S, almacenamiento o integridad de los datos guardados",
      },
      {
        code: "5",
        meaning: "Límite de recursos o capacidad/archivo no admitido",
      },
    ],
    limitsTitle: "Límites seleccionados del MVP",
    limitsCaption:
      "Límites de recursos de Limits and state en el README; no son mediciones de rendimiento",
    limitHeaders: ["Recurso", "Límite"],
    limits: [
      { resource: "Archivo de origen", limit: "2 MiB" },
      {
        resource: "Archivos / bytes de origen por búsqueda",
        limit: "20.000 / 100 MiB",
      },
      { resource: "Coincidencias por resultado", limit: "20.000" },
      { resource: "Archivos por plan", limit: "50" },
      {
        resource: "Bytes conjuntos de antes/después por plan",
        limit: "20 MiB",
      },
      { resource: "Salida JSON, incluido salto de línea", limit: "64 KiB" },
      { resource: "Límite opcional de tokens", limit: "256–1.000.000" },
      { resource: "TTL de resultados / planes sin aplicar", limit: "24 horas" },
      {
        resource: "Cuota del estado persistente del repositorio",
        limit: "200 MiB",
      },
      { resource: "Notas activas / texto por nota", limit: "1.024 / 8 KiB" },
      { resource: "Claves y texto de las notas activas", limit: "4 MiB" },
      { resource: "Caché separada del índice por defecto", limit: "512 MiB" },
    ],
  },
  recovery: {
    eyebrow: "05 — SEGURIDAD Y ESTADO",
    title: ["Comprobación de conflictos", "y recuperación explícita."],
    description:
      "Las transacciones de redb protegen el estado interno, no crean una transacción atómica sobre archivos externos. Estas restricciones forman parte del contrato documentado.",
    items: [
      {
        title: "Antes de la primera escritura",
        description:
          "apply comprueba todos los hashes y permisos básicos. Persiste snapshots, un checkpoint y un journal por archivo antes de reemplazar el código. Un conflicto previo no modifica archivos de origen.",
      },
      {
        title: "Reemplazo secuencial de archivos",
        description:
          "Un fallo intermedio puede dejar algunos archivos cambiados. No hay rollback automático. Una recuperación pendiente bloquea nuevos planes, apply y undo; siguen disponibles la inspección y los diagnósticos.",
      },
      {
        title: "Undo y recibos históricos",
        description:
          "undo comprueba todos los hashes y permisos posteriores antes de restaurar el checkpoint completo. Las repeticiones devuelven recibos históricos, no prueban el estado actual. Una operación confirmada se deshace con undo, no con rollback de recuperación.",
      },
      {
        title: "Escritores concurrentes",
        description:
          "El bloqueo coordina procesos n4h, no editores ni otros escritores. Sigue existiendo una carrera entre el último hash y rename. Usa un worktree dedicado sin escrituras concurrentes; no es un sandbox para árboles hostiles.",
      },
      {
        title: "Código de origen persistido",
        description:
          "El estado está en .n4h/state.redb y el índice opcional con contenido completo en .n4h/index.redb. Notas, vistas de coincidencias, bytes de antes/después y checkpoints pueden contener secretos. No hay cifrado, borrado seguro ni modo --no-store. Excluye .n4h/ de Git manualmente.",
      },
      {
        title: "Caducidad y compatibilidad",
        description:
          "El TTL invalida referencias, no borra filas ni checkpoints. No hay recolección automática. El estado SQLite antiguo se rechaza con legacy_state_detected, no se importa automáticamente. macOS, Windows, sistemas de archivos de red y durabilidad ante pérdida de energía no están certificados.",
      },
    ],
    inspectTitle: "Inspeccionar operaciones pendientes",
    inspectNote:
      "Sin --rollback, recover solo inspecciona. Sustituye op_OPERATION por el ID real y sigue los cursores devueltos antes de decidir cómo continuar.",
    rollbackTitle: "Rollback explícito de una operación pendiente",
    rollbackNote:
      "Este comando restaura bytes de origen. Solo acepta las versiones y permisos esperados de antes/después. Los cambios inesperados producen recovery_conflict. No hay sobrescritura forzada: conserva .n4h/ y reconcilia los cambios externos en lugar de borrar checkpoints.",
    scopeTitle: "No implementado en este MVP",
    scopeDescription:
      "Son exclusiones del alcance actual, no compromisos de publicación.",
    exclusions: [
      "Renombrado semántico, refactorización con LSP o tipos, ni AST de lenguajes distintos de Rust.",
      "Consultas regex/AST indexadas, watcher/daemon de archivos ni compactación automática de la caché.",
      "Embeddings, memoria semántica, captura automática de conversaciones ni MCP.",
      "Tokenizadores personalizados, detección automática de modelo/tokenizador, estimaciones de facturación ni un benchmark de modelos/agentes.",
      "Comandos de retención, exportación o limpieza del estado operativo, ni soporte multiplataforma de producción.",
    ],
  },
  reproducibility: {
    demosTitle: "Demos con escenarios aislados",
    demosDescription:
      "Python 3 es necesario para estos drivers, no para n4h. Crean sus propios escenarios y capturan IDs reales. El modo AST opcional --check-rust necesita además rustc y solo comprueba el escenario de la demo.",
    checksTitle: "Compilación y comprobaciones",
    checksDescription:
      "Estos comandos están documentados en Build. --all-features se usa para pruebas; no lo uses con cargo install porque activa la inyección de fallos.",
    indexTitle: "Benchmark sintético del índice",
    indexDescription:
      "Usa un escenario aislado, comprueba la igualdad de resultados en vivo/indexados, alterna el orden de consultas y registra muestras, mediana y rango. También verifica actualizaciones sin cambios, de un archivo y con --verify.",
    limitsTitle: "Alcance de la medición",
    limitations: [
      "Los tiempos incluyen arranque del proceso, salida JSON y persistencia del resultado; no se vacían las cachés del sistema operativo.",
      "El corpus es texto sintético repetitivo y la consulta es selectiva. Los resultados son exploratorios, no garantizan una aceleración general.",
      "No se miden memoria máxima, tokens del modelo, consumo del proveedor ni coste total de tareas del agente.",
    ],
  },
};

export const referenceCopy = { en, es };
