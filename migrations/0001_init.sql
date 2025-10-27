-- Run once (idempotency for indexes not guaranteed across versions).
CREATE TABLE IF NOT EXISTS countries (
  id                INT AUTO_INCREMENT PRIMARY KEY,
  name              VARCHAR(128) NOT NULL,
  capital           VARCHAR(128) NULL,
  region            VARCHAR(64)  NULL,
  population        BIGINT       NOT NULL,
  currency_code     CHAR(3)      NULL,
  exchange_rate     DOUBLE       NULL,
  estimated_gdp     DOUBLE       NULL,
  flag_url          VARCHAR(256) NULL,
  last_refreshed_at DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP,
  UNIQUE KEY ux_countries_name (name)
);

CREATE INDEX idx_countries_region ON countries (region);
CREATE INDEX idx_countries_currency ON countries (currency_code);
CREATE INDEX idx_countries_gdp ON countries (estimated_gdp);

CREATE TABLE IF NOT EXISTS app_meta (
  k VARCHAR(64) PRIMARY KEY,
  v VARCHAR(512) NOT NULL
);
