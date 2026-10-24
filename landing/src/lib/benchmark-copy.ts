const en = {
  title: "CLI task comparison",
  attribution: "RESULTS REPORTED BY THE TEAM",
  description:
    "11 search, edit, undo, concurrency, and session-resume tasks per condition, repeated five times. n4h used the checksum-verified v0.1.0 release binary in an isolated bench/bin/ directory.",
  runs: "passes per condition",
  tasks: "tasks per pass",
  caption:
    "Results per pass of 11 tasks. Counts were identical across five passes; only elapsed time varied.",
  metric: "Metric",
  scrollHint:
    "On small screens, scroll the table horizontally to compare all three conditions.",
  conditions: {
    shell: "Classic shell",
    ripgrep: "Shell + ripgrep",
    n4h: "n4h 0.1.0",
  },
  metrics: {
    estimatedTokens: "Estimated tokens (≈ bytes/4)",
    toolCalls: "Tool calls",
    defectiveEdits: "Defective edits",
    completed: "Tasks completed",
    safeRefusals: "Safe refusals · no writes",
    incorrectAnswers: "Incorrect / incomplete answers",
    timeMs: "Total time · median [range]",
  },
  tokenNote:
    "Token figures are a bytes/4 estimate, not exact tokenization, model input/output usage, or provider billing. Safe refusals are reported separately and do not count as completed tasks.",
  findingsTitle: "Reading the results",
  findings: [
    {
      title: "Edits and conflicts",
      description:
        "No defective edits were observed with n4h, versus one in each shell condition. It refused both undo cases (U1/U2) without writing; these refusals did not complete the requested task. C1 detected an external change automatically.",
    },
    {
      title: "Output and time",
      description:
        "n4h produced about 7.3× the estimated tokens and needed more tool calls and time in this suite. The report attributes much of the output to per-match metadata and pagination for counts/aggregations. S4 emitted less output but was incomplete.",
    },
    {
      title: "Coverage and session context",
      description:
        "S3/S4 were incorrect or incomplete because n4h excluded access.log (≈2.37 MB), above its 2 MiB per-file limit, and reported it in skipped. In R1, mem and resume recovered the result_id without repeating the search.",
    },
  ],
  methodologyTitle: "Methodology and limitations",
  limitations: [
    "Each condition ran five passes of the same 11 tasks. Counts and outcomes did not vary; the table shows per-pass totals and median [minimum–maximum] elapsed time, not totals over 55 tasks.",
    "These are team-reported CLI task results, not a measurement of an LLM’s token consumption or a guarantee of safety, latency, or cost on other repositories.",
    "Coverage is not equivalent: n4h’s documented file-size policy excluded access.log. The two safe refusals and two incorrect/incomplete answers remain separate from its seven completed tasks.",
    "No unintended access.log modification was reported in any condition. This does not remove the coverage limitation above.",
    "The full corpus, exact commands, environment, baseline versions, and raw run artifacts have not been attached to this page. This comparison was not independently rerun for the landing.",
  ],
  repositoryExamplesTitle: "Separate: repository demos and index benchmark",
  repositoryExamplesDescription:
    "The commands below come from the README. They are not the harness for the 11-task comparison above; index_bench.py measures a different synthetic indexing scenario.",
};

const es: typeof en = {
  title: "Comparativa de tareas CLI",
  attribution: "RESULTADOS REPORTADOS POR EL EQUIPO",
  description:
    "11 tareas de búsqueda, edición, undo, concurrencia y reanudación por condición, repetidas cinco veces. Se usó el binario de la release n4h v0.1.0, con checksum verificado y aislado en bench/bin/.",
  runs: "pasadas por condición",
  tasks: "tareas por pasada",
  caption:
    "Resultados por pasada de 11 tareas. Los recuentos fueron iguales en las cinco pasadas; solo varió el tiempo.",
  metric: "Métrica",
  scrollHint:
    "En pantallas pequeñas, desliza la tabla horizontalmente para comparar las tres condiciones.",
  conditions: {
    shell: "Shell clásico",
    ripgrep: "Shell + ripgrep",
    n4h: "n4h 0.1.0",
  },
  metrics: {
    estimatedTokens: "Tokens estimados (≈ bytes/4)",
    toolCalls: "Llamadas a herramientas",
    defectiveEdits: "Ediciones defectuosas",
    completed: "Tareas completadas",
    safeRefusals: "Rechazos seguros · sin escrituras",
    incorrectAnswers: "Respuestas incorrectas / incompletas",
    timeMs: "Tiempo total · mediana [rango]",
  },
  tokenNote:
    "Los tokens se estiman con bytes/4: no son una tokenización exacta, consumo de entrada/salida del modelo ni facturación del proveedor. Los rechazos seguros se muestran aparte y no cuentan como tareas completadas.",
  findingsTitle: "Cómo leer los resultados",
  findings: [
    {
      title: "Ediciones y conflictos",
      description:
        "No se observaron ediciones defectuosas con n4h, frente a una en cada condición shell. Rechazó ambos casos de undo (U1/U2) sin escribir; esos rechazos no completaron la tarea solicitada. En C1 detectó automáticamente un cambio externo.",
    },
    {
      title: "Salida y tiempo",
      description:
        "n4h produjo unas 7,3 veces más tokens estimados y necesitó más llamadas y tiempo en esta suite. El reporte atribuye gran parte de la salida a metadatos por coincidencia y paginación para contar o agregar. S4 emitió menos salida, pero quedó incompleta.",
    },
    {
      title: "Cobertura y contexto de sesión",
      description:
        "S3/S4 fueron incorrectas o incompletas porque n4h excluyó access.log (≈2,37 MB), que supera su límite de 2 MiB por archivo, y lo informó en skipped. En R1, mem y resume recuperaron el result_id sin repetir la búsqueda.",
    },
  ],
  methodologyTitle: "Metodología y limitaciones",
  limitations: [
    "Cada condición ejecutó cinco pasadas de las mismas 11 tareas. Los recuentos y resultados no variaron; la tabla muestra totales por pasada y mediana [mínimo–máximo] del tiempo, no totales de 55 tareas.",
    "Son resultados de tareas CLI reportados por el equipo, no una medición del consumo de tokens de un LLM ni una garantía de seguridad, latencia o coste en otros repositorios.",
    "La cobertura no es equivalente: la política de tamaño de n4h excluyó access.log. Los dos rechazos seguros y las dos respuestas incorrectas/incompletas quedan separados de sus siete tareas completadas.",
    "No se reportaron modificaciones involuntarias de access.log en ninguna condición. Esto no elimina la limitación de cobertura anterior.",
    "Esta página aún no adjunta el corpus completo, los comandos exactos, el entorno, las versiones de referencia ni los archivos de resultados. La comparativa no se volvió a ejecutar de forma independiente para la landing.",
  ],
  repositoryExamplesTitle:
    "Por separado: demos y benchmark del índice del repositorio",
  repositoryExamplesDescription:
    "Los comandos siguientes proceden del README. No son el harness de la comparativa de 11 tareas anterior; index_bench.py mide otro escenario sintético de indexación.",
};

export const benchmarkCopy = { en, es };
