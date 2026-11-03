# Contract: the CSV exports (D12)

Each export is `<stem>.csv` plus `<stem>.sidecar.txt` (001's sidecar of the render the figures come from).

```
# rr-dr60-workbench <view> 1
# engine 0.92.0 a06318ca…, rate 48000 Hz, <method parameters>
<header row>
<rows>
```

| View | Method line | Header |
|---|---|---|
| spectrum | `N 9600, Hann periodic, 50 % overlap, power average, dB re full-scale sine per 5 Hz bin (ENBW 7.5 Hz); range in [a, b) out [c, d); frames original k, render m (dropped j)` | `freq_hz,original_db,render_db,ratio_db` (empty where a side is outside the 120 dB window) |
| meters | `block 480 (10 ms); spans in host samples; kinds max/count/state` | `row,in_start,in_end,out_start,out_end,tail,` + every field's identifier |
| taps | `fresh engine per tap; readings as 001's` | `tap,status,length,latency,tail,splices_or_regions,not_emitted,peak,sha256` |
