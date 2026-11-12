# Trial 2: Word's own "Save as PDF" of the made-up text in expected.json, with a table, in a hidden
# Word of its own (it quits only that one). Run: powershell -File make_word.ps1 <out folder>
param([string]$out)
if (Get-Process WINWORD -ErrorAction SilentlyContinue) { Write-Output "Word is open: not touching it"; exit 1 }
$log = Join-Path $out 'make_word.log'
function Step($m) { Add-Content -Path $log -Value "$(Get-Date -Format HH:mm:ss) $m" }
Step 'start'
$t = Get-Content -Raw -Encoding UTF8 (Join-Path $out 'expected.json') | ConvertFrom-Json
$w = New-Object -ComObject Word.Application
$w.Visible = $false
$w.DisplayAlerts = 0
Step 'word started'
try {
    $d = $w.Documents.Add()
    Step 'document added'
    $r = $d.Content
    $r.Text = $t.title
    $r.Style = $d.Styles.Item(-2) # Heading 1
    $r.ParagraphFormat.ReadingOrder = 0 # right to left
    foreach ($p in $t.paragraphs) {
        $r.InsertParagraphAfter()
        $r = $d.Paragraphs.Item($d.Paragraphs.Count).Range
        $r.Text = $p
        $r.Style = $d.Styles.Item(-1) # Normal
        $r.ParagraphFormat.ReadingOrder = $(if ($p -match '^[A-Za-z]') { 1 } else { 0 })
    }
    Step 'paragraphs written'
    # a small table: reading order inside cells
    $d.Content.InsertParagraphAfter()
    $end = $d.Paragraphs.Item($d.Paragraphs.Count).Range
    $tbl = $d.Tables.Add($end, 2, 2)
    $tbl.Borders.Enable = $true
    $tbl.Cell(1, 1).Range.Text = 'العمود الأول'
    $tbl.Cell(1, 2).Range.Text = 'Second column'
    $tbl.Cell(2, 1).Range.Text = 'خلية فيها 42 كلمة'
    $tbl.Cell(2, 2).Range.Text = 'a cell, with عربي'
    $tbl.Rows.Item(1).Range.ParagraphFormat.ReadingOrder = 0
    Step 'table written'
    $d.SaveAs2([string](Join-Path $out 'word.pdf'), 17)
    $d.Close(0)
    Step 'exported'
    Write-Output "made: word.pdf"
} finally {
    $w.Quit(0)
    [void][Runtime.InteropServices.Marshal]::ReleaseComObject($w)
}
