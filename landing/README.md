# n4h — Landing

Presentación técnica bilingüe del README de n4h, en blanco y negro, con Next.js App Router, React y TypeScript. Independiente de la compilación de Rust. No ejecuta la CLI ni los scripts del proyecto desde la web.

## Desarrollo

Requiere Node.js 20.9 o posterior.

```sh
cd landing
npm ci
npm run dev
```

Abre http://localhost:3000. La raíz redirige a `/en`; la versión en español está en `/es`.

```sh
npm run typecheck
npm run build
```

## Vercel

Importa el repositorio en Vercel y configura **Root Directory** como `landing`. El framework se detecta como Next.js; no hacen falta variables de entorno ni servicios adicionales. Esta carpeta no compila ni ejecuta el binario Rust.

Las URLs canónicas usan el dominio de producción que Vercel proporciona en `VERCEL_PROJECT_PRODUCTION_URL`. Si usas un dominio propio, puedes definir opcionalmente `NEXT_PUBLIC_SITE_URL` con su URL completa (por ejemplo, `https://tu-dominio.com`). Fuera de Vercel, configura esa variable al desplegar para que las URLs canónicas no apunten a localhost.

## Idiomas

- `/en`: inglés (predeterminado).
- `/es`: español.
- El selector **EN / ES** está disponible en escritorio y móvil; no se fuerza un idioma según el navegador.
- Los textos, metadatos y descripciones accesibles están en `src/lib/i18n.ts`. Ambos idiomas comparten el mismo diseño en `src/app/[lang]/page.tsx`.
- Los comandos y los identificadores técnicos son compartidos; las explicaciones y los comentarios del terminal se traducen.
- Cada ruta publica su atributo `lang`, URL canónica y enlaces `hreflang` a las versiones alternativas.

## Documentación en la landing

El README del proyecto (`../README.md`) es la fuente del contenido. La página resume el flujo de edición, los modos de búsqueda, el contrato de salida, los límites, la persistencia y la recuperación. Cada capacidad y sección de referencia enlaza a su apartado del README. No se publican promesas de rendimiento, ahorro de tokens ni compatibilidad que no estén documentadas.

La sección `#documentation` incluye un quick start bilingüe con instalación desde código fuente, inicialización del repositorio, búsqueda, flujo de edición, límites de salida con `--max-tokens` / `--tokenizer` y memoria explícita con `mem` / `resume`. Los bloques pueden copiarse; las rutas y los IDs marcados como ejemplos deben sustituirse antes de ejecutarlos. El límite de tokens mide el JSON de salida con el tokenizador elegido; no es una estimación de facturación ni una promesa de ahorro. `resume` consulta contexto, no ejecuta tareas.

Los comandos y enlaces a las guías del README están en `src/lib/content.ts`; las explicaciones están en `src/lib/i18n.ts` y la referencia técnica ampliada en `src/lib/reference-copy.ts`. Mantén estas instrucciones alineadas con el README del proyecto. Son resúmenes y traducciones mantenidos explícitamente, no una sincronización automática del Markdown.

La sección de reproducibilidad incluye los comandos documentados de compilación, pruebas, demos aisladas y benchmark del índice. Explica el alcance y las limitaciones de las mediciones; no añade resultados numéricos de ejemplo.

Para comprobar el contenido servido, inicia un servidor de producción y ejecuta:

```sh
npm run test:docs -- http://localhost:3094
```

La prueba verifica ambas rutas, las secciones técnicas, los comandos, los límites seleccionados y que los enlaces de origen correspondan a encabezados del README local. No ejecuta n4h ni valida sus garantías o rendimiento.

## Publicar benchmarks

Edita `src/lib/content.ts`:

- `taskBenchmarkReport` contiene la comparativa CLI proporcionada por el equipo: 11 tareas por condición y cinco pasadas con n4h v0.1.0 (`d65fd47`). Los recuentos son por pasada; los tiempos son mediana y rango de las cinco pasadas. Los tokens son una estimación `bytes/4`, no una tokenización exacta ni consumo del modelo. No se volvió a ejecutar esta comparativa para la landing.
- Los textos bilingües de la comparativa están en `src/lib/benchmark-copy.ts`. Mantén visibles los rechazos seguros, los fallos de cobertura de archivos mayores de 2 MiB y las limitaciones de metodología. No cuentes un rechazo seguro como una tarea completada.
- Esta comparación es independiente de `scripts/index_bench.py`. Las demos y el benchmark sintético del índice se muestran por separado; sus comandos no reproducen la suite de 11 tareas. Aún no se adjuntaron corpus, entorno, comandos exactos ni archivos de resultados de esa suite.

- Para futuras mediciones del benchmark sintético del índice, añade resultados reales al array `benchmarks`: nombre (`name: { en, es }`), resultado de n4h, referencia de comparación y contexto (`context: { en, es }`, incluye unidades). Los valores medidos se comparten entre idiomas.
- Añade en `benchmarkMethodologyUrl` el enlace a la metodología, versiones, entorno y escenarios reproducibles.
- La tabla adicional de mediciones del índice aparece al añadir resultados a `benchmarks`; no sustituye ni se mezcla con la comparación de tareas CLI.

Los bloques de terminal son ejemplos estáticos interactivos; no ejecutan la CLI. Sus identificadores son marcadores de posición. El comando de instalación requiere haber clonado el repositorio.

Los textos están en `src/lib/i18n.ts`; los comandos de ejemplo, benchmarks y enlaces están en `src/lib/content.ts`. Las fuentes se cargan desde Google Fonts con fuentes de sistema de respaldo.
