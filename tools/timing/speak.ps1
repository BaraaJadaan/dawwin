# Makes the recordings of the timing bench with the voices of Windows, and when each word is said.
#
#   pwsh tools/timing/speak.ps1 [-Out <folder>]
#
# For every recording in talks.json: <name>.wav (16 kHz mono) and <name>.truth.json, the words in
# the order they are said with the moment each one starts (from the voice itself, not a guess).
# The talks stop in the middle of sentences, for 0.3 to 2.5 seconds, and change speed from one
# phrase to the next, as people do. A voice that is not installed is skipped with a note.
param([string]$Out = (Join-Path $PSScriptRoot "out"))

$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Speech
$speech = [System.Speech.Synthesis.SpeechSynthesizer].Assembly.Location
Add-Type -ReferencedAssemblies $speech, "System.Collections", "System.Runtime", "System.Private.CoreLib", "netstandard" -TypeDefinition @"
using System;
using System.Collections.Generic;
using System.Speech.Synthesis;
using System.Speech.AudioFormat;

public static class TimingVoice {
    public class Said { public string text; public int char_at; public double start_s; }

    public static List<Said> Speak(string voice, string ssml, string wav) {
        var said = new List<Said>();
        using (var s = new SpeechSynthesizer()) {
            s.SelectVoice(voice);
            s.SetOutputToWaveFile(wav, new SpeechAudioFormatInfo(16000, AudioBitsPerSample.Sixteen, AudioChannel.Mono));
            s.SpeakProgress += (o, e) => said.Add(new Said { text = e.Text, char_at = e.CharacterPosition, start_s = e.AudioPosition.TotalSeconds });
            s.SpeakSsml(ssml);
            s.SetOutputToNull();
        }
        return said;
    }

    public static bool Has(string voice) {
        using (var s = new SpeechSynthesizer()) {
            foreach (var v in s.GetInstalledVoices()) if (v.Enabled && v.VoiceInfo.Name == voice) return true;
        }
        return false;
    }
}
"@

function Escape([string]$t) { [System.Security.SecurityElement]::Escape($t) }

New-Item -ItemType Directory -Force $Out | Out-Null
$talks = Get-Content -Raw -Encoding utf8 (Join-Path $PSScriptRoot "talks.json") | ConvertFrom-Json
foreach ($talk in $talks) {
    if (-not [TimingVoice]::Has($talk.voice)) { Write-Host "skipped $($talk.name): no voice $($talk.voice)"; continue }
    $body = ""
    foreach ($p in $talk.parts) {
        $body += "<prosody rate=`"$($p.rate)`">$(Escape $p.text)</prosody>"
        if ($p.pause) { $body += "<break time=`"$([int]($p.pause * 1000))ms`"/>" }
        $body += " "
    }
    $ssml = "<speak version=`"1.0`" xmlns=`"http://www.w3.org/2001/10/synthesis`" xml:lang=`"$($talk.culture)`">$body</speak>"
    $wav = Join-Path $Out "$($talk.name).wav"
    $said = [TimingVoice]::Speak($talk.voice, $ssml, $wav)
    $truth = [ordered]@{
        name = $talk.name
        language = $talk.language
        voice = $talk.voice
        text = (($talk.parts | ForEach-Object { $_.text }) -join " ")
        words = @($said | ForEach-Object { [ordered]@{ text = $_.text; start = [math]::Round($_.start_s, 3) } })
    }
    $truth | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 (Join-Path $Out "$($talk.name).truth.json")
    Write-Host "$($talk.name): $($said.Count) words, $([math]::Round((Get-Item $wav).Length / 32000, 1)) s"
}
