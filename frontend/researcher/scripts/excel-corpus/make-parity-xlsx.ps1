param([string]$SpecFile, [string]$Out)
$ErrorActionPreference = 'Stop'
$spec = Get-Content -Raw -Encoding UTF8 $SpecFile | ConvertFrom-Json
$xl = New-Object -ComObject Excel.Application
$xl.Visible = $false
$xl.DisplayAlerts = $false
try {
  $wb = $xl.Workbooks.Add()
  while ($wb.Worksheets.Count -lt $spec.sheets.Count) { [void]$wb.Worksheets.Add([Type]::Missing, $wb.Worksheets.Item($wb.Worksheets.Count)) }
  for ($i = 0; $i -lt $spec.sheets.Count; $i++) { $wb.Worksheets.Item($i + 1).Name = $spec.sheets[$i].name }
  foreach ($n in $spec.names) { [void]$wb.Names.Add($n.name, $n.ref) }
  foreach ($s in $spec.sheets) {
    $ws = $wb.Worksheets.Item($s.name)
    foreach ($c in $s.cells) {
      $cell = $ws.Range($c.a)
      if ($null -ne $c.f) { $cell.Formula = $c.f }
      else {
        $v = $c.v
        if ($v -is [string]) { $cell.Value2 = [string]$v } else { $cell.Value2 = [double]$v }
      }
    }
    if ($s.name -eq "Staff") { $ws.Range("D2:D11").NumberFormat = "yyyy-mm-dd" }
  }
  $xl.Calculate()
  $wb.SaveAs($Out, 51)
  $wb.Close($false)
} finally { $xl.Quit(); [void][System.Runtime.Interopservices.Marshal]::ReleaseComObject($xl) }
'saved'
