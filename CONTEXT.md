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
