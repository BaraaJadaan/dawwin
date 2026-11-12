# Trial 2: "Microsoft Print to PDF", as any program that prints makes a PDF: the made-up text drawn
# with GDI+ (Windows shapes the Arabic), printed to a file. The default printer is not changed.
# Run in Windows PowerShell: powershell -File make_print.ps1 <out folder>
param([string]$out)
Add-Type -AssemblyName System.Drawing
$t = Get-Content -Raw -Encoding UTF8 (Join-Path $out 'expected.json') | ConvertFrom-Json
$blocks = @($t.title) + @($t.paragraphs)
$doc = New-Object System.Drawing.Printing.PrintDocument
$doc.PrinterSettings.PrinterName = 'Microsoft Print to PDF'
if (-not $doc.PrinterSettings.IsValid) { Write-Output "no Microsoft Print to PDF"; exit 1 }
$doc.PrinterSettings.PrintToFile = $true
$doc.PrinterSettings.PrintFileName = (Join-Path $out 'mspdf.pdf')
$doc.DocumentName = $t.title
$doc.add_PrintPage({
    param($s, $e)
    $font = New-Object System.Drawing.Font('Segoe UI', 13)
    $y = 60
    foreach ($b in $blocks) {
        $rtl = -not ($b -match '^[A-Za-z]')
        $fmt = New-Object System.Drawing.StringFormat
        if ($rtl) { $fmt.FormatFlags = [System.Drawing.StringFormatFlags]::DirectionRightToLeft }
        $rect = New-Object System.Drawing.RectangleF(60, $y, 680, 200)
        $size = $e.Graphics.MeasureString($b, $font, 680, $fmt)
        $e.Graphics.DrawString($b, $font, [System.Drawing.Brushes]::Black, $rect, $fmt)
        $y += $size.Height + 18
    }
    $e.HasMorePages = $false
})
$doc.Print()
Start-Sleep -Seconds 2
Write-Output "made: mspdf.pdf"
