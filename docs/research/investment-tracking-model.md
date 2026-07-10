# Modelo de seguimiento de inversiones para Tracky

Fecha de investigación: 2026-07-10

## Pregunta

¿Debe Tracky tratar como gastos las compras de dólares o activos referenciados al dólar, los CDT y las acciones adquiridas mediante trii, o puede ampliar su modelo para mostrar cuánto se invierte cada mes y qué inversiones se mantienen?

## Conclusión ejecutiva

**Tracky sí debería gestionar inversiones, pero como un subdominio acotado de finanzas personales, no como un broker, asesor de inversión ni motor tributario.** La compra de un activo no debería contaminar el reporte de consumo: cambia la composición del patrimonio, pero no lo reduce por el principal completo. Lo que sí puede reducirlo son comisiones, impuestos, pérdidas realizadas u otros costos.

No es necesario resolver precios en tiempo real para obtener valor desde la primera versión. Un MVP puede responder dos preguntas distintas:

1. **Flujo mensual:** cuánto capital nuevo se destinó a inversiones, cuánto se retiró y cuánto se recibió por intereses o dividendos.
2. **Posición:** qué activos se tienen, en qué cuenta, cuántas unidades, cuál fue su costo y a qué fecha se conoce su valor.

Si Tracky sólo observa una salida bancaria, no puede deducir de forma fiable qué activo se compró. Debe permitir confirmar primero «dinero destinado a inversión» y dejar la asignación a un activo como pendiente hasta recibir información del usuario o un extracto de la plataforma. El fallback correcto no es convertirlo en gasto, sino conservarlo como **aporte de inversión pendiente de detalle**, excluido del gasto de consumo.

## Hechos comprobados en fuentes oficiales

Esta sección describe hechos; las decisiones de producto aparecen después.

### 1. Adquirir un activo no equivale necesariamente a incurrir en un gasto

El Marco Conceptual de la IFRS Foundation distingue los elementos de posición financiera —activos, pasivos y patrimonio— de los elementos de desempeño —ingresos y gastos—. Define un activo como un recurso económico presente controlado y señala explícitamente que existen intercambios de activos que no aumentan ni reducen el patrimonio. También define gasto como una disminución de activos o aumento de pasivos que sí reduce el patrimonio. Por esa razón conceptual, cambiar COP por USD, un CDT o acciones no convierte por sí solo todo el principal en gasto. [IFRS Foundation, Marco Conceptual, capítulo 4](https://www.ifrs.org/content/dam/ifrs/publications/pdf-standards/english/2021/issued/part-a/conceptual-framework-for-financial-reporting.pdf)

El mismo marco indica que una variación del valor actual puede contener componentes diferentes, por ejemplo cambio de precio e interés acumulado, y que separarlos puede producir información más útil. Esto respalda distinguir principal, rendimientos y valorización. Esta fuente se usa como marco conceptual, no como afirmación de que una aplicación personal deba preparar estados financieros bajo NIIF.

### 2. Divisa USD y activos digitales referenciados al dólar no son lo mismo

El Banco de la República incluye la tenencia, adquisición o disposición de activos en divisas dentro de las operaciones de cambio y explica que en Colombia las tasas de compra y venta son acordadas por las partes; no existe un precio estatal único para negociar divisas. [Banco de la República, conceptos básicos de operaciones cambiarias](https://www.banrep.gov.co/es/politica-monetaria-cambiaria/regulacion-operaciones-cambiarias/conceptos-basicos) y [concepto sobre inexistencia de una tasa oficial](https://www.banrep.gov.co/es/banco/junta-directiva/conceptos/jds-ca-08482)

La TRM es un indicador diario calculado y certificado por la Superintendencia Financiera a partir de operaciones del mercado; no es necesariamente la tasa efectiva a la que una plataforma comprará o venderá los dólares del usuario. [Banco de la República, definición de TRM](https://www.banrep.gov.co/es/glosario/tasa-cambio-trm)

Para fines fiscales, la DIAN ha explicado que un activo en moneda extranjera tiene un reconocimiento inicial en COP y que usar dólares para adquirir posteriormente acciones cambia la naturaleza del activo: deja de ser efectivo en USD y pasa a ser otro activo. [DIAN, Oficio 907958 de 2021](https://normograma.dian.gov.co/dian/compilacion/docs/oficio_dian_907958_2021.htm)

Wenia ofrece USDC, que su documentación define como un criptoactivo estable diseñado para mantener una referencia 1:1 con el USD, y aclara que no es divisa ni dinero ni tiene respaldo gubernamental. El flujo que publica consiste en adquirir COPW y después convertirlo a USDC, validando montos y comisiones. [Wenia, guía oficial de USDC](https://www.wenia.com/es/productos/criptoactivos/usdc)

Este hecho **no demuestra qué activo produjo una transacción bancaria concreta con descripción Wenia**. Esa salida podría haber quedado en COPW, haberse convertido a USDC o corresponder a otra operación disponible en la plataforma. Para conocer la posición exacta hacen falta movimientos de Wenia o confirmación del usuario sobre activo, unidades y comisión. Tracky no debe etiquetar automáticamente esos movimientos como USD fiat ni como USDC.

### 3. CDT: principal, plazo, intereses, retención y renovación son datos distintos

La Superintendencia Financiera describe el CDT como un depósito de dinero a un plazo y una tasa determinados; además, indica que no se redime antes del vencimiento, aunque el título puede negociarse. [Superintendencia Financiera, glosario de CDT](https://www.superfinanciera.gov.co/publicaciones/13226/glosario-c-13226/) y [pregunta frecuente sobre redención anticipada](https://www.superfinanciera.gov.co/preguntas-frecuentes/8/8-certificado-de-deposito-a-termino-cdt/)

La estructura oficial de información exógena de la DIAN para certificados a término separa número de título, tipo de movimiento, saldo inicial, inversión efectuada, intereses causados, intereses pagados, retención y saldo final. También dice que una renovación no constituye por sí sola un nuevo depósito o una nueva inversión; sólo se reportan como adición los rendimientos capitalizados o el capital adicional. [DIAN, Resolución 162 de 2023, Formato 1020](https://normograma.dian.gov.co/dian/compilacion/docs/resolucion_dian_0162_2023.htm)

### 4. trii separa efectivo, posiciones, movimientos, rendimientos y costos

La documentación oficial de trii distingue el **saldo en caja** del **portafolio de inversión**. También indica que la app muestra por separado depósitos, retiros, acciones negociadas y fondos; por tanto, depositar dinero en trii no prueba que ya se hayan comprado acciones. El valor del portafolio depende del mercado. [trii, preguntas frecuentes](https://www.trii.co/faq)

La misma FAQ señala que:

- las compras en la app se hacen en COP;
- actualmente las acciones se compran en unidades enteras;
- los dividendos llegan al saldo disponible;
- existen comisiones de compra y venta, un cobro sobre dividendos y retenciones en la fuente;
- una venta puede generar ganancia o pérdida respecto de la inversión inicial.

Las tarifas publicadas pueden cambiar, por lo que Tracky debería guardar los importes reales de cada operación y no codificar valores de comisión en el dominio. [trii, tarifas oficiales](https://www.trii.co/)

### 5. Los soportes del proveedor siguen siendo la fuente autoritativa

La DIAN recomienda conservar certificados de inversiones, certificados de rendimientos financieros y certificados de dividendos. Esto refuerza que los extractos o certificados de Wenia, el banco emisor del CDT y trii/Acciones & Valores deben tener provenance propia si se importan a Tracky. [DIAN, documentos soporte para renta de personas naturales 2025](https://micrositios.dian.gov.co/renta-personas-naturales-ag-2025/como-hacer-su-declaracion/)

## Estado actual de Tracky

Hechos observados en el repositorio:

- `CONTEXT.md` ya define una cuenta como un contenedor con tipo y moneda propios y pone como ejemplos broker y wallet cripto. También define gasto como una reducción real del patrimonio y transferencia como un movimiento entre cuentas propias.
- `migrations/0001_review_first_schema.sql` registra una sola moneda y un solo importe por transacción canónica; las líneas sólo admiten `expense`.
- Las transferencias propias actuales enlazan transacciones monetarias, pero no representan la adquisición de unidades de un activo ni un intercambio entre monedas.
- El PRD inicial excluyó explícitamente el modelado multimoneda de inversiones/cripto. Por tanto, añadirlo es una ampliación deliberada de alcance y no una simple categoría nueva.

## Recomendación de producto

### Límite del subdominio

Tracky debería ser la **vista consolidada y reconciliable** del dinero personal:

- sí: aportes, retiros, compras, ventas, vencimientos, posiciones, costo, valor observado, intereses, dividendos, comisiones, retenciones y provenance;
- no inicialmente: ejecutar órdenes, recomendar instrumentos, calcular obligaciones tributarias, obtener precios en tiempo real, pronosticar rendimientos o reemplazar los extractos/certificados del proveedor.

Este límite mantiene el propósito original —automatizar la comprensión de ingresos y gastos— y añade la pregunta natural «¿qué parte de mi dinero no se consumió sino que quedó invertida?».

### No usar `Inversiones` como categoría de gasto

Los reportes deberían tener, como mínimo, cuatro bloques separados:

1. **Gastos de consumo.**
2. **Capital destinado a inversiones.**
3. **Ingresos de inversiones:** intereses y dividendos brutos.
4. **Costos de inversión:** comisiones y otros cargos, con retenciones identificadas aparte.

Una retención en la fuente no debería degradarse silenciosamente a «comisión» o «consumo». Tracky puede mostrarla como deducción/retención asociada al rendimiento y dejar el tratamiento tributario definitivo fuera de alcance.

### Modelo conceptual mínimo

No es necesario convertir toda la base existente en un libro contable general. Conviene añadir una capa de operaciones de inversión enlazada a las transacciones canónicas actuales.

#### Cuenta de inversión

Reutiliza el concepto actual de cuenta propia. Ejemplos:

- `Wenia — saldo de activos digitales` — cuenta de proveedor; sus instrumentos internos se determinan desde el extracto o confirmación, no desde el débito bancario;
- `trii — saldo en caja` — efectivo COP aún no invertido;
- `trii — portafolio` — cuenta custodia de instrumentos;
- `CDT <entidad>` — contrato o cuenta a término.

#### Instrumento

Identidad estable de lo poseído:

- efectivo USD;
- USDC u otro activo digital, únicamente cuando esté confirmado;
- CDT específico;
- acción identificada por ticker/emisor/mercado.

Campos transversales recomendados: tipo, moneda de denominación, institución/emisor e identificador del proveedor. Para un CDT se agregan fecha de constitución, vencimiento, principal, tasa y modalidad de pago. No todos los instrumentos necesitan los mismos campos.

#### Operación de inversión

Evento revisable con provenance que agrupa componentes relacionados:

- fecha de operación y, cuando aplique, fecha de liquidación;
- cuenta e instrumento;
- tipo: aporte, retiro, compra, venta, conversión FX, constitución, vencimiento/redención, interés, dividendo, comisión o retención;
- efectivo entregado/recibido con moneda;
- cantidad del activo y precio unitario, si aplican;
- vínculo a una o más transacciones canónicas;
- estado de revisión y fuente.

Los importes brutos y sus deducciones deben conservarse por separado. Así, una venta o un vencimiento puede reconciliar principal, rendimiento, comisión, retención y efectivo neto sin perder información.

#### Posición derivada

La posición no debería editarse como un saldo aislado si puede calcularse de las operaciones confirmadas:

- cantidad mantenida;
- costo acumulado y costo promedio informativo;
- principal vigente para CDT;
- ganancias/pérdidas realizadas en ventas;
- fecha del último movimiento.

Tracky debe admitir una corrección o snapshot de proveedor cuando la historia importada sea incompleta, siempre conservando la provenance y la diferencia de conciliación.

#### Snapshot de valoración

Una observación fechada, nunca un «valor actual» sin contexto:

- cantidad;
- precio o tasa;
- valor en moneda del instrumento;
- valor convertido a COP;
- fuente y fecha/hora;
- indicador de dato observado o estimado.

Para USD fiat pueden coexistir costo histórico, valor de referencia con TRM y valor de liquidación estimado con la tasa de venta del proveedor. Un activo digital referenciado al USD requiere el precio de ese activo y no debe valorarse como si fuera automáticamente USD fiat. Para acciones, la fuente primaria inicial puede ser el extracto/valor mostrado por trii. Para CDT, debe mostrarse por separado principal, intereses acumulados estimados y valor esperado al vencimiento; no debe presentarse como efectivo inmediatamente disponible.

## Reglas por tipo de inversión

### Compra de USD fiat o activo referenciado al USD

Registrar una conversión con dos cantidades: COP entregados y unidades del activo recibidas. La tasa efectiva se deriva de esas cantidades; las comisiones se registran aparte si están disponibles. El instrumento debe distinguir `USD` de `USDC`, `COPW` u otro activo digital.

- Aporte mensual nuevo: costo COP de las unidades adquiridas con dinero externo al portafolio.
- Tenencia: cantidad restante por instrumento.
- Venta/uso: disminuye unidades y registra COP recibido o el destino dado al activo.
- Rendimiento: diferencia realizada al vender; la variación de valoración mientras se mantiene debe mostrarse como no realizada.

### CDT

Al constituirlo, mover el principal desde la cuenta bancaria al instrumento CDT. Guardar los términos contractuales.

Al vencimiento, separar:

- devolución del principal: no es ingreso;
- interés bruto: ingreso de inversión;
- retención u otras deducciones;
- efectivo neto recibido.

Una renovación no cuenta otra vez como inversión mensual. Sólo cuenta el capital nuevo agregado; el interés capitalizado se identifica como reinversión de rendimiento, no como dinero nuevo del usuario.

### Acciones mediante trii

Modelar al menos dos saldos distintos:

1. saldo en caja de trii;
2. posiciones por acción.

El depósito PSE desde Nequi a trii es una transferencia/aporte a una cuenta de inversión, no una compra de acciones. La compra posterior reduce saldo en caja y aumenta unidades de la acción; la comisión queda separada. El dividendo aumenta saldo en caja como ingreso bruto menos cargo/retención. La venta reduce unidades, aumenta saldo en caja, registra comisión y permite calcular ganancia o pérdida realizada. Retirar de trii a Nequi sólo mueve efectivo propio y no crea ingreso.

## Definición de los reportes mensuales

Para evitar dobles conteos, «invertí este mes» no debería ser un único total ambiguo. Se recomiendan estas métricas:

- **Capital nuevo aportado:** efectivo que salió de cuentas cotidianas hacia cuentas/instrumentos de inversión, menos devoluciones que anulen el aporte.
- **Compras/constituciones brutas:** valor de todas las adquisiciones del mes, incluyendo reinversiones.
- **Reinversión:** compras financiadas con ventas, vencimientos, intereses o dividendos ya dentro del portafolio.
- **Retiros de capital:** principal que volvió a cuentas cotidianas.
- **Aporte neto:** capital nuevo aportado menos retiros de capital.
- **Ingresos de inversión:** intereses y dividendos brutos.
- **Costos y retenciones:** mostrados por separado.
- **Valor de cierre:** última valoración disponible por posición, con fecha y estado de frescura.

La métrica principal para la pregunta del usuario debería ser **capital nuevo aportado**. Depositar COP en trii y luego comprar acciones con esos mismos COP no puede sumar dos veces. Las compras brutas se muestran como desglose de asignación, no como capital adicional.

## Manejo de información incompleta

El flujo de revisión debe aceptar niveles de conocimiento:

1. **Intención confirmada:** el usuario confirma que la salida fue inversión.
2. **Aporte conciliado:** se conoce origen, destino, fecha y monto.
3. **Asignación conocida:** se identifica instrumento y cantidad.
4. **Posición conciliada:** operaciones acumuladas coinciden con extracto/certificado.
5. **Valoración disponible:** existe un snapshot fechado.

Esto evita inventar posiciones. Una salida confirmada como inversión puede afectar de inmediato el reporte mensual de capital aportado y quedar como `asignación pendiente`; no debe volver a gasto sólo porque falte el detalle del instrumento.

Las descripciones o contrapartes conocidas pueden producir sugerencias futuras, pero no aceptación automática. Un pago a una plataforma puede ser inversión, retiro de deuda, comisión, compra de un servicio u otra operación; la intención sigue requiriendo evidencia o confirmación.

## Implementación por fases

### Fase 1 — aportes y tenencias al costo

- Clasificar salidas confirmadas como aporte/inversión, no gasto.
- Registrar cuentas de inversión e instrumentos manualmente.
- Capturar cantidad, costo, principal y términos básicos.
- Reportar capital nuevo mensual, posiciones al costo, ingresos y costos.
- Permitir asignación pendiente cuando sólo se conoce el aporte.

Esta fase ya responde «cuánto invertí cada mes» y «qué tengo registrado» sin depender de precios externos.

### Fase 2 — extractos y conciliación

- Importar documentos de Wenia, CDT y trii como candidatos review-first.
- Conciliar depósitos/retiros bancarios con operaciones del proveedor.
- Derivar posiciones y detectar diferencias contra snapshots del extracto.

### Fase 3 — valoraciones y rendimiento

- Importar o consultar precios/tasas con fuente y fecha.
- Separar rendimiento realizado y no realizado.
- Añadir alertas de valoración desactualizada.

### Fuera de alcance hasta que exista una necesidad concreta

- ejecución de órdenes;
- recomendaciones de inversión;
- optimización de portafolio;
- cálculo o presentación automática de impuestos;
- precios intradía obligatorios;
- derivados, cripto y métodos fiscales avanzados de lotes.

## Decisión propuesta

1. **No clasificar como gasto el principal confirmado de USD, CDT o acciones.**
2. **Extender Tracky con aportes y posiciones de inversión por fases.**
3. **Empezar por costo y provenance, no por valor en vivo.**
4. **Mantener consumo, capital invertido, rendimientos y costos como métricas separadas.**
5. **Cuando falte el detalle, conservar `inversión pendiente de asignación` en vez de inventar una tenencia o degradarla a gasto.**

La complejidad es moderada y controlable si Tracky no intenta ser un broker ni un sistema tributario. El beneficio es alto: preservar cifras confiables de ingresos/gastos y, a la vez, explicar dónde quedó el dinero que no fue consumido.
