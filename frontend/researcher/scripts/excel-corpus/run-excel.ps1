# Evaluate every formula of cases.mjs in Microsoft Excel (desktop, Windows) through COM and
# write the results as JSON. Used to refresh src/lib/workspace/formula/fixtures/excel-corpus.json.
#
#   node -e "import('./scripts/excel-corpus/cases.mjs').then(m => require('fs').writeFileSync('input.json', JSON.stringify(m)))"
#   powershell -File scripts/excel-corpus/run-excel.ps1 -In input.json -Out excel-out.json
#   node scripts/excel-corpus/build-corpus.mjs excel-out.json
#
# Each formula is entered in Data!J20 with Formula2 (dynamic arrays), so spilled results are
# read from the spill range. The machine locale should be en-US.
param([string]$In, [string]$Out)
$ErrorActionPreference = 'Stop'
$data = Get-Content -Raw -Encoding UTF8 $In | ConvertFrom-Json
$xl = New-Object -ComObject Excel.Application
$xl.Visible = $false
$xl.DisplayAlerts = $false
$xl.ScreenUpdating = $false
$errMap = @{2000='#NULL!';2007='#DIV/0!';2015='#VALUE!';2023='#REF!';2029='#NAME?';2036='#NUM!';2042='#N/A';2043='#GETTING_DATA';2045='#SPILL!';2046='#CONNECT!';2047='#BLOCKED!';2048='#UNKNOWN!';2049='#FIELD!';2050='#CALC!'}
function Conv($v) {
  if ($null -eq $v) { return $null }
  if ($v -is [int]) { $n = $v + 2146828288; if ($errMap.ContainsKey($n)) { return @{ error = $errMap[$n] } } else { return @{ error = "#ERR$n" } } }
  return $v
}
try {
  $wb = $xl.Workbooks.Add()
  $sheetNames = @($data.grid.PSObject.Properties.Name)
  while ($wb.Worksheets.Count -lt $sheetNames.Count) { [void]$wb.Worksheets.Add([Type]::Missing, $wb.Worksheets.Item($wb.Worksheets.Count)) }
  for ($i = 0; $i -lt $sheetNames.Count; $i++) { $wb.Worksheets.Item($i + 1).Name = $sheetNames[$i] }
  foreach ($sn in $sheetNames) {
    $ws = $wb.Worksheets.Item($sn)
    $rows = $data.grid.$sn
    for ($r = 0; $r -lt $rows.Count; $r++) {
      $row = $rows[$r]
      if ($null -eq $row) { continue }
      for ($c = 0; $c -lt @($row).Count; $c++) {
        $v = @($row)[$c]
        if ($null -eq $v) { continue }
        $cell = $ws.Cells.Item($r + 1, $c + 1)
        if ($v -is [string] -and $v.StartsWith('=')) { $cell.Formula = $v }
        elseif ($v -is [string]) { $cell.Value2 = [string]$v }
        elseif ($v -is [bool]) { $cell.Value2 = [bool]$v }
        else { $cell.Value2 = [double]$v }
      }
    }
  }
  foreach ($n in $data.names) { [void]$wb.Names.Add($n.name, $n.expression) }
  $ws = $wb.Worksheets.Item('Data')
  $target = $ws.Range('J20')
  $results = New-Object System.Collections.ArrayList
  foreach ($f in $data.cases) {
    $entry = @{ f = $f }
    try {
      $ws.Range('J20:Z60').ClearContents() | Out-Null
      $target.Formula2 = $f
      $xl.Calculate()
      $hasSpill = $false
      try { $hasSpill = [bool]$target.HasSpill } catch {}
      if ($hasSpill) {
        $rng = $target.SpillingToRange
        $vals = $rng.Value2
        $arr = New-Object System.Collections.ArrayList
        for ($r = 1; $r -le $rng.Rows.Count; $r++) {
          $line = New-Object System.Collections.ArrayList
          for ($c = 1; $c -le $rng.Columns.Count; $c++) { [void]$line.Add((Conv $vals[$r, $c])) }
          [void]$arr.Add($line)
        }
        $entry.array = $arr
      } else {
        $entry.value = Conv $target.Value2
        $entry.text = [string]$target.Text
      }
    } catch {
      $entry.parseError = $_.Exception.Message
    }
    [void]$results.Add($entry)
  }
  $json = $results | ConvertTo-Json -Depth 8 -Compress
  [System.IO.File]::WriteAllText($Out, $json, (New-Object System.Text.UTF8Encoding $false))
  $wb.Close($false)
} finally {
  $xl.Quit()
  [void][System.Runtime.Interopservices.Marshal]::ReleaseComObject($xl)
}
'done'
