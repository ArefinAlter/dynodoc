-- Extend scoped provenance credentials; existing grants and histories remain unchanged.
ALTER TABLE connector_grant DROP CONSTRAINT connector_grant_host_check;
ALTER TABLE connector_grant ADD CONSTRAINT connector_grant_host_check
  CHECK (host IN ('word', 'google-docs', 'excel', 'google-sheets', 'powerpoint', 'google-slides'));
