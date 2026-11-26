# Painel de diagnóstico do Galocego

O painel lateral mostra os tempos de quadro e das etapas de renderização, o uso de memória e as últimas amostras no console integrado. O painel é desenhado no quadro apresentado e aparece nas capturas internas quando está aberto.

```powershell
cargo build --locked --profile diag -p victory --features diagnostics,agility
$env:VICTORY_HUD_SCALE = '2'
wscript.exe .\Play-HUD.vbs '<executável local do título>'
```

O launcher também aceita o executável do Victory como segundo argumento. Use **F9** para mostrar ou ocultar o painel e `VICTORY_HUD_SCALE` de `1` a `3` para definir o tamanho inicial. O launcher grava a saída em uma pasta local sob `%LOCALAPPDATA%\Galocego\Logs`.

O console integrado usa apenas contadores de desempenho. Para relatar um teste publicamente, informe o commit, a cena, o intervalo medido e a mediana do tempo por quadro. Revise os logs brutos e as capturas localmente antes de compartilhar qualquer dado.
