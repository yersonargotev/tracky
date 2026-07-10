# Tracky Domain Context

## Glossary

### Transacción candidata

Una transacción extraída o propuesta por una fuente no final, como un PDF, una IA o un importador, que aún requiere revisión antes de afectar los datos financieros canónicos.

### Transacción canónica

Una transacción confirmada por el usuario que forma parte del registro financiero oficial de Tracky.

### Provenance

La evidencia que permite rastrear de dónde salió una transacción candidata o canónica, incluyendo fuente, archivo, página, texto extraído, importador, confianza y lote de importación.

### Línea de transacción

Una parte categorizable de una transacción. Una transacción simple tiene una sola línea; una transacción dividida tiene varias líneas cuya suma debe coincidir con el monto total de la transacción.

### Split

Una transacción representada por varias líneas de transacción para análisis más fino, como separar comida, domicilio, propina y comisión dentro de una misma compra.

### Institución financiera

La entidad de origen o administración de una o más cuentas financieras, como Nequi, Rappi, Bancolombia, Nu, Trii o Binance.

### Cuenta

Un contenedor financiero específico dentro de una institución, con tipo y moneda propios, como una billetera, cuenta de ahorros, tarjeta de crédito, broker, wallet cripto o efectivo.

### Gasto

Una transacción que reduce el patrimonio neto del usuario por consumo, compra, comisión, interés, impuesto u otro egreso real.

### Ingreso

Una transacción que aumenta el patrimonio neto del usuario, como nómina, intereses, dividendos, reembolsos confirmados o ventas.

### Transferencia

Un movimiento entre cuentas propias que no debe contarse como gasto ni ingreso, como pagar una tarjeta de crédito Rappi o Nu desde Nequi.

### Aporte de inversión

Una salida confirmada de capital destinada a inversión. El principal no es gasto de consumo ni ingreso y permanece ligado a su cuenta de origen, fecha, monto, moneda, descripción y provenance.

### Asignación pendiente

Estado explícito de un aporte de inversión cuyo instrumento o cantidad adquirida todavía no se conoce. Tracky conserva el aporte sin inventar una posición ni degradarlo a gasto.

### Instrumento de inversión

Identidad estable de un activo adquirido. El tipo, moneda de denominación, proveedor o emisor e identificador del proveedor distinguen activos que no son intercambiables; USD fiat, USDC y COPW son instrumentos diferentes.

### Asignación de inversión

Detalle confirmado que vincula parte del principal de un aporte con una cantidad exacta de un instrumento. Puede ser parcial y sus correcciones conservan revisiones append-only.

### Posición a costo histórico

Cantidad y costo acumulados derivados de asignaciones activas por cuenta, instrumento y moneda del costo. No es un saldo editable ni una valoración de mercado.

### Posición CDT

Principal vigente y términos contractuales derivados de la constitución y de las renovaciones o redenciones activas de un instrumento `fixed_income`. Conserva por separado capital externo, interés capitalizado, interés bruto, principal retornado, retenciones, otras deducciones y efectivo neto; nunca es un saldo editable.

### Operación CDT

Evento append-only de constitución, renovación o redención. Cada corrección crea una revisión y mueve un active head, de modo que el historial, la asignación o aporte financiador y la provenance manual sigan reconstruibles.

### Documento fuente

Un archivo importado, como un extracto bancario PDF, del que se extraen transacciones candidatas. Debe tener una huella de archivo para evitar reimportaciones exactas.

### Huella de transacción

Una clave calculada a partir de datos normalizados de una transacción, como cuenta, fecha, monto, descripción y fuente, usada para detectar posibles duplicados entre documentos distintos.

### Posible duplicado

Una transacción candidata que se parece a una transacción existente o a otra candidata, pero requiere revisión humana antes de aceptarse o rechazarse.

### Fuente de ingreso

El origen económico de un ingreso, como empleador, cliente freelance, dividendos, intereses o reembolso. Es independiente de la cuenta donde se recibe el dinero.

### Credencial de documento

Una contraseña u otro secreto necesario para abrir un documento fuente protegido. Tracky puede recibirla en tiempo de ejecución desde CLI, prompt interactivo o variables de entorno cargadas desde un archivo `.env`, pero no la almacena como dato canónico en la versión inicial.

### Cuenta de brokerage

Cuenta propia de custodia cuya caja disponible por moneda y posiciones en instrumentos `security` se derivan de operaciones activas. Un depósito consume capital externo confirmado una sola vez; compras, ventas, dividendos y retiros cambian la composición de ese capital sin crear saldos editables.

### Operación de brokerage

Evento append-only de depósito, compra, venta, dividendo o retiro. Conserva cantidades exactas, costo histórico, producto bruto, resultado realizado, fees, retenciones, otras deducciones, efectivo neto y provenance; una corrección crea otra revisión y mueve el active head.
