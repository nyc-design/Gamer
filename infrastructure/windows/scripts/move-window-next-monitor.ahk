#Requires AutoHotkey v2.0

; Ctrl+Alt+Right -> move active window to next monitor, preserving relative position.
^!Right:: {
    WinGetPos &x, &y, &w, &h, "A"
    mCount := MonitorGetCount()
    if (mCount < 2)
        return

    mon := MonitorGetPrimary()
    ; best-effort monitor detection by window center
    cx := x + (w // 2)
    cy := y + (h // 2)
    loop mCount {
        i := A_Index
        MonitorGet i, &l, &t, &r, &b
        if (cx >= l && cx <= r && cy >= t && cy <= b) {
            mon := i
            break
        }
    }

    next := mon + 1
    if (next > mCount)
        next := 1

    MonitorGet mon, &l1, &t1, &r1, &b1
    MonitorGet next, &l2, &t2, &r2, &b2

    relX := x - l1
    relY := y - t1

    WinMove l2 + relX, t2 + relY, w, h, "A"
}
