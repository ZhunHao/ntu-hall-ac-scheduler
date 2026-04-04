/**
 * @file ac_scheduler.ino
 * @brief ESP32 Daikin AC Scheduler v0.1 — Asymmetric Thermal Protocol.
 * * Vacation mode pauses the auto-schedule, but manually turning the AC ON
 * automatically cancels Vacation Mode.
 * * UI Update: Rearranged flow for optimal UX (Controls Top -> Timers ->
 * Schedules Bottom).
 * * OTA PASSWORD: "admin"
 */

#include <Adafruit_BME280.h>
#include <Adafruit_Sensor.h>
#include <Arduino.h>
#include <ArduinoOTA.h>
#include <ESPmDNS.h>
#include <IRremoteESP8266.h>
#include <IRsend.h>
#include <Preferences.h>
#include <WebServer.h>
#include <WiFi.h>
#include <WiFiManager.h>
#include <Wire.h>
#include <ir_Daikin.h>
#include <time.h>

// =============================================================================
// Reference: Network & Identity
// =============================================================================
// WiFi credentials are managed by WiFiManager — no hardcoding needed.
// On first boot, connect to the setup AP and enter your credentials.
// To re-configure, hit GET /reset-wifi from the browser.

const char *deviceName    = "AC-Scheduler";
const char *wifiApName    = "AC-Scheduler-Setup";   // AP shown during portal
const char *wifiApPass    = "acsetup01";             // Portal AP password (min 8 chars)
const int   portalTimeout = 180;              // seconds before AP closes

// =============================================================================
// Explanation: Operational State Variables
// =============================================================================
const uint16_t kIrLed = D1;
const long gmtOffset_sec = 28800; // GMT+8
const int daylightOffset_sec = 0;

const int SDA_PIN = 22;
const int SCL_PIN = 23;
Adafruit_BME280 bme;
bool bmeActive = false;

IRDaikinESP ac(kIrLed);
WebServer server(80);
Preferences preferences;

char systemStatus[25] = "Standby";
int currentTemp = 24;
int currentSlotIndex = 0;

// Cycle tracking variables
bool lastCycleWasOn = false;
bool inActiveWindow = false;

unsigned long timerStartMillis = 0;
unsigned long timerDuration = 0;
bool timerActive = false;
int activeTimerMinutes = 0;

// Vacation Logic Variables
long vacationStartDate = 0;
long vacationEndDate = 0;
bool manualVacation = false;
bool vacationMode = false;

// Fix 9-A: Set by /cmd and /timer when the user takes manual control.
// Cleared at the 07:00 window boundary so the next night resumes auto.
bool manualOverride = false;

// Mutex: guards shared state modified by both loop() and web handlers.
// IR send bursts are performed OUTSIDE the lock (they block for ~1s).
SemaphoreHandle_t stateMux = NULL;

// =============================================================================
// Fix 11+12: Schedule table + helpers (replaces magic-number if-chain x3)
// To shift/add/remove cycles, edit ONLY this table.
// =============================================================================
struct CycleSlot {
  int startMin;
  int endMin;
};
const CycleSlot SCHEDULE[] = {
    {22 * 60, 22 * 60 + 45},      // Thermal Purge    22:00-22:45 (45m)
    {23 * 60 + 15, 23 * 60 + 40}, // Sleep Onset      23:15-23:40 (25m)
    {0 * 60 + 15, 0 * 60 + 35},   // Deep Sleep Entry 00:15-00:35 (20m)
    {1 * 60 + 20, 1 * 60 + 40},   // Deep Sleep Hold  01:20-01:40 (20m)
    {2 * 60 + 40, 3 * 60 + 0},    // Cold Window      02:40-03:00 (20m)
    {4 * 60 + 15, 4 * 60 + 40},   // Pre-Dawn Prep    04:15-04:40 (25m)
    {5 * 60 + 20, 6 * 60 + 0},    // Sunrise Fort     05:20-06:00 (40m)
};
const int SCHEDULE_LEN = sizeof(SCHEDULE) / sizeof(SCHEDULE[0]);

// Returns true if the current hour falls in the 22:00-07:00 active window.
bool isNightWindow(const struct tm &t) {
  return (t.tm_hour >= 22 || t.tm_hour < 7);
}

// Returns true if minutesSinceMidnight falls inside any ON slot.
bool shouldAcBeOn(int m) {
  for (int i = 0; i < SCHEDULE_LEN; i++) {
    if (m >= SCHEDULE[i].startMin && m < SCHEDULE[i].endMin)
      return true;
  }
  return false;
}

// =============================================================================
// Explanation: Web Interface (SPA Architecture)
// =============================================================================
const char index_html[] PROGMEM = R"rawliteral(
<!DOCTYPE HTML><html><head>
<meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1,maximum-scale=1,user-scalable=0">
<title>AC Scheduler</title>
<style>
*{box-sizing:border-box;margin:0;padding:0}
:root{
  --bg:#f2f2f7;--glass:rgba(255,255,255,0.72);--glass-border:rgba(0,0,0,0.06);
  --text1:#1c1c1e;--text2:#8e8e93;--fill:rgba(120,120,128,0.12);
  --accent:#007aff;--warm:#ff9f0a;--cool:#64d2ff;--red:#ff3b30;--green:#34c759;
  --shadow:0 2px 16px rgba(0,0,0,0.08);--shadow-lg:0 8px 32px rgba(0,0,0,0.12);
}
[data-theme="dark"]{
  --bg:#000;--glass:rgba(44,44,46,0.72);--glass-border:rgba(255,255,255,0.08);
  --text1:#fff;--text2:#98989d;--fill:rgba(120,120,128,0.24);
  --accent:#0a84ff;--red:#ff453a;--green:#30d158;
  --shadow:0 2px 16px rgba(0,0,0,0.3);--shadow-lg:0 8px 32px rgba(0,0,0,0.4);
}
body{background:var(--bg);color:var(--text1);font-family:-apple-system,BlinkMacSystemFont,"SF Pro Display","SF Pro Rounded",sans-serif;margin:0;padding:16px;transition:background 0.4s;-webkit-tap-highlight-color:transparent}
.wrap{max-width:500px;margin:0 auto}
.glass{background:var(--glass);backdrop-filter:blur(20px);-webkit-backdrop-filter:blur(20px);border-radius:22px;border:1px solid var(--glass-border);box-shadow:var(--shadow)}
.hero{padding:28px 24px 24px;margin-bottom:16px;text-align:left;position:relative;overflow:hidden;transition:background 0.5s,box-shadow 0.5s}
.hero-icon{font-size:1.6rem;margin-bottom:8px;display:block;filter:grayscale(1);opacity:0.5;transition:filter 0.4s,opacity 0.4s}
.hero[data-state="cold"] .hero-icon,.hero[data-state="warm"] .hero-icon{filter:grayscale(0);opacity:1}
.hero[data-state="cold"]{background:linear-gradient(135deg,rgba(100,210,255,0.18),var(--glass));box-shadow:var(--shadow-lg),0 0 40px rgba(100,210,255,0.1)}
.hero[data-state="warm"]{background:linear-gradient(135deg,rgba(255,159,10,0.22),var(--glass));box-shadow:var(--shadow-lg),0 0 40px rgba(255,159,10,0.12)}
.hero-temp{font-size:5.5rem;font-weight:200;line-height:1;letter-spacing:-3px;margin:4px 0 6px;transition:opacity 0.4s}
.hero-status{font-size:1.05rem;font-weight:600;opacity:0.7;margin-bottom:2px}
.hero-room{font-size:0.85rem;color:var(--text2);display:none}
.hero-timer{position:absolute;bottom:20px;right:20px;font-family:ui-monospace,SFMono-Regular,monospace;font-size:0.8rem;font-weight:600;background:rgba(0,0,0,0.15);color:#fff;padding:5px 12px;border-radius:14px;display:none;backdrop-filter:blur(8px)}
.hero[data-state="off"] .hero-timer{background:var(--fill);color:var(--text2)}
@keyframes pulse{0%,100%{box-shadow:var(--shadow-lg)}50%{box-shadow:var(--shadow-lg),0 0 48px rgba(100,210,255,0.15)}}
.hero[data-state="cold"].active{animation:pulse 3s ease-in-out infinite}
@keyframes pulse-warm{0%,100%{box-shadow:var(--shadow-lg)}50%{box-shadow:var(--shadow-lg),0 0 48px rgba(255,159,10,0.18)}}
.hero[data-state="warm"].active{animation:pulse-warm 3s ease-in-out infinite}
.tiles{display:grid;grid-template-columns:1fr 1fr;gap:12px;margin-bottom:16px}
.tile{padding:20px 16px;text-align:left;cursor:pointer;transition:transform 0.1s,background 0.3s,box-shadow 0.3s;min-height:110px;display:flex;flex-direction:column;justify-content:space-between}
.tile:active{transform:scale(0.96)}
.tile-icon{font-size:1.8rem;margin-bottom:auto;filter:grayscale(1);opacity:0.5;transition:filter 0.3s,opacity 0.3s}
.tile-label{font-size:0.85rem;font-weight:600;color:var(--text2);margin-top:12px;transition:color 0.3s}
.tile.active .tile-icon{filter:grayscale(0);opacity:1}
.tile.t-cold.active{background:linear-gradient(150deg,rgba(100,210,255,0.2),var(--glass))}
.tile.t-cold.active .tile-label{color:#32ade6}
.tile.t-warm.active{background:linear-gradient(150deg,rgba(255,159,10,0.2),var(--glass))}
.tile.t-warm.active .tile-label{color:#ff9f0a}
.tile.t-off.active{background:linear-gradient(150deg,rgba(255,59,48,0.12),var(--glass))}
.tile.t-off.active .tile-label{color:var(--red)}
.tile.t-timer.active{background:linear-gradient(150deg,rgba(52,199,89,0.15),var(--glass))}
.tile.t-timer.active .tile-label{color:var(--green)}
.sec-hdr{font-size:0.7rem;font-weight:600;color:var(--text2);text-transform:uppercase;letter-spacing:0.6px;padding:16px 4px 8px;text-align:left}
.timer-card{padding:20px;margin-bottom:16px}
.preset-row{display:grid;grid-template-columns:repeat(4,1fr);gap:8px;margin-bottom:16px}
.preset{background:var(--fill);border:none;border-radius:12px;padding:12px 0;font-size:0.85rem;font-weight:600;color:var(--text2);cursor:pointer;transition:all 0.2s;font-family:inherit}
.preset:active{transform:scale(0.95)}
.preset.active{background:var(--accent);color:#fff;box-shadow:0 2px 12px rgba(0,122,255,0.3)}
.custom-row{display:flex;justify-content:center;align-items:flex-end;gap:12px;margin-bottom:16px}
.custom-col{display:flex;flex-direction:column;align-items:center}
.custom-lbl{font-size:0.7rem;color:var(--text2);margin-bottom:6px;font-weight:500;text-transform:uppercase;letter-spacing:0.3px}
.custom-input{width:80px;height:70px;font-size:3.2rem;font-weight:200;background:var(--fill);border:2px solid transparent;border-radius:14px;color:var(--text1);text-align:center;font-family:inherit;font-variant-numeric:tabular-nums;outline:none;transition:0.2s}
.custom-input:focus{border-color:var(--warm);background:rgba(255,159,10,0.1);box-shadow:0 0 0 4px rgba(255,159,10,0.15)}
input[type=number]::-webkit-outer-spin-button,input[type=number]::-webkit-inner-spin-button{-webkit-appearance:none;margin:0}
input[type=number]{-moz-appearance:textfield}
.custom-sep{font-size:3rem;font-weight:200;color:var(--text2);padding-bottom:6px}
.btn-start{width:100%;padding:14px;border:none;border-radius:14px;font-size:0.95rem;font-weight:600;background:var(--green);color:#fff;cursor:pointer;font-family:inherit;transition:transform 0.1s,opacity 0.3s}
.btn-start:active{transform:scale(0.97)}
.sched-card{padding:4px 0;margin-bottom:16px}
.sched-row{display:flex;align-items:center;padding:14px 20px;gap:12px}
.sched-row+.sched-row{border-top:1px solid var(--glass-border)}
.sched-dot{width:8px;height:8px;border-radius:50%;background:var(--fill);flex-shrink:0;transition:background 0.3s}
.sched-row.active .sched-dot{background:var(--accent);box-shadow:0 0 8px rgba(0,122,255,0.4)}
.sched-time{font-size:0.9rem;font-weight:600;flex:1}
.sched-desc{font-size:0.8rem;color:var(--text2);text-align:right}
.vac-card{padding:0;margin-bottom:16px;overflow:hidden}
.vac-header{display:flex;align-items:center;justify-content:space-between;padding:16px 20px}
.vac-label{font-size:0.9rem;font-weight:600}
.vac-sub{font-size:0.75rem;color:var(--text2);font-weight:400}
.ios-toggle{width:51px;height:31px;border-radius:16px;background:var(--fill);position:relative;cursor:pointer;transition:background 0.3s;flex-shrink:0;border:none}
.ios-toggle::after{content:'';position:absolute;width:27px;height:27px;border-radius:50%;background:#fff;top:2px;left:2px;box-shadow:0 1px 3px rgba(0,0,0,0.2);transition:transform 0.25s cubic-bezier(0.4,0,0.2,1)}
.ios-toggle.on{background:var(--green)}
.ios-toggle.on::after{transform:translateX(20px)}
.vac-dates{display:flex;gap:12px;padding:0 20px 16px;flex-wrap:wrap}
.vac-date-col{flex:1;min-width:120px}
.vac-date-lbl{font-size:0.7rem;color:var(--text2);margin-bottom:6px;font-weight:500;text-transform:uppercase;letter-spacing:0.3px}
input[type=date]{-webkit-appearance:none;-moz-appearance:none;appearance:none;display:block;width:100%;min-height:42px;border:none;border-radius:10px;padding:10px 12px;font-family:inherit;font-size:16px;background:var(--fill);color:var(--text1);outline:none;box-sizing:border-box;line-height:1.4}
input[type=date]::-webkit-date-and-time-value{text-align:left}
.vac-save{width:100%;padding:13px;border:none;border-top:1px solid var(--glass-border);background:transparent;font-size:0.9rem;font-weight:600;color:var(--accent);cursor:pointer;font-family:inherit;transition:background 0.2s,color 0.2s}
.vac-save:active{background:var(--fill)}
.vac-save.saved{color:var(--green)}
.seg{display:inline-flex;border-radius:18px;padding:2px;background:var(--fill);margin:24px auto 16px;position:relative}
.seg-btn{border:none;background:none;color:var(--text2);padding:7px 18px;font-size:0.8rem;font-weight:600;border-radius:16px;cursor:pointer;font-family:inherit;transition:all 0.25s}
.seg-btn.active{background:var(--glass);color:var(--text1);box-shadow:0 1px 4px rgba(0,0,0,0.1)}
</style>
</head>
<body data-theme="light">
<div class="wrap">
  <div id="hero" class="glass hero" data-state="off">
    <div class="hero-name">My Room</div>
    <span class="hero-icon" id="hero-icon">&#10052;</span>
    <div class="hero-temp" id="temp">--</div>
    <div class="hero-status" id="st">Connecting...</div>
    <div class="hero-room" id="real-weather"></div>
    <div class="hero-timer" id="td">00:00</div>
  </div>
  <div class="tiles">
    <div id="tile-cold" class="glass tile t-cold" onclick="send('on16')">
      <span class="tile-icon">&#10052;</span>
      <span class="tile-label">Max Cool<br><small style="font-weight:400;opacity:0.7">16&deg;C</small></span>
    </div>
    <div id="tile-warm" class="glass tile t-warm" onclick="send('on25')">
      <span class="tile-icon">&#9728;&#65039;</span>
      <span class="tile-label">Comfort<br><small style="font-weight:400;opacity:0.7">25&deg;C</small></span>
    </div>
    <div id="tile-off" class="glass tile t-off" onclick="send('off')">
      <span class="tile-icon">&#9211;</span>
      <span class="tile-label">Turn Off</span>
    </div>
    <div id="tile-timer" class="glass tile t-timer">
      <span class="tile-icon">&#9201;</span>
      <span class="tile-label" id="timer-tile-label">No Timer</span>
    </div>
  </div>
  <div class="sec-hdr">Timer</div>
  <div class="glass timer-card">
    <div class="preset-row">
      <button id="tm30" class="preset" onclick="setT(30)">30m</button>
      <button id="tm60" class="preset" onclick="setT(60)">1h</button>
      <button id="tm120" class="preset" onclick="setT(120)">2h</button>
      <button id="tm240" class="preset" onclick="setT(240)">4h</button>
    </div>
    <div class="custom-row">
      <div class="custom-col">
        <span class="custom-lbl">Hr</span>
        <input type="number" id="mac-hr" class="custom-input" value="00" min="0" max="23" oninput="formatInput(this,23)" onblur="padZero(this)">
      </div>
      <span class="custom-sep">:</span>
      <div class="custom-col">
        <span class="custom-lbl">Min</span>
        <input type="number" id="mac-min" class="custom-input" value="00" min="0" max="59" oninput="formatInput(this,59)" onblur="padZero(this)">
      </div>
    </div>
    <button id="tm-custom" class="btn-start" onclick="setCustom()">Start Custom Timer</button>
  </div>
  <div class="sec-hdr">Schedule</div>
  <div class="glass sched-card">
    <div id="slot1" class="sched-row"><span class="sched-dot"></span><span class="sched-time">22:00 &ndash; 07:00</span><span class="sched-desc">195m ON / Asymmetric</span></div>
    <div id="slot2" class="sched-row"><span class="sched-dot"></span><span class="sched-time">07:00 &ndash; 22:00</span><span class="sched-desc">Standby</span></div>
  </div>
  <div class="sec-hdr">Vacation Mode</div>
  <div class="glass vac-card">
    <div class="vac-header">
      <div><div class="vac-label" id="vac-label">Off</div><div class="vac-sub" id="vac-sub">Tap to pause schedule</div></div>
      <button id="btn-vacation" class="ios-toggle" onclick="toggleVacation()"></button>
    </div>
    <div class="vac-dates">
      <div class="vac-date-col"><div class="vac-date-lbl">Departure</div><input type="date" id="d-start"></div>
      <div class="vac-date-col"><div class="vac-date-lbl">Return</div><input type="date" id="d-end"></div>
    </div>
    <button id="btn-cal" class="vac-save" onclick="saveCal()">Set Schedule</button>
  </div>
  <div style="text-align:center">
    <div class="seg">
      <button id="t-light" class="seg-btn" onclick="setTheme('light')">Light</button>
      <button id="t-dark" class="seg-btn" onclick="setTheme('dark')">Dark</button>
      <button id="t-auto" class="seg-btn active" onclick="setTheme('auto')">Auto</button>
    </div>
  </div>
</div>
<script>
let currentMode='auto';
// Fix 14: Singapore sunrise/sunset — 01°22'N, variation ±15min across year.
const sunData={sunrise:7.0,sunset:19.17};
const dStart=document.getElementById('d-start'),dEnd=document.getElementById('d-end');

function formatInput(el,mx){el.value=el.value.replace(/[^0-9]/g,'');if(el.value.length>2)el.value=el.value.slice(-2);if(parseInt(el.value)>mx)el.value=mx}
function padZero(el){if(el.value==="")el.value="00";else if(el.value.length===1)el.value="0"+el.value}
document.getElementById('mac-hr').addEventListener('focus',function(){if(this.value==="00")this.value=""});
document.getElementById('mac-min').addEventListener('focus',function(){if(this.value==="00")this.value=""});

function applyAutoTheme(){if(currentMode!=='auto')return;const n=new Date(),h=n.getHours()+(n.getMinutes()/60);document.body.setAttribute('data-theme',(h>=sunData.sunrise&&h<sunData.sunset)?'light':'dark')}
function setTheme(m){currentMode=m;document.querySelectorAll('.seg-btn').forEach(b=>b.classList.remove('active'));document.getElementById('t-'+m).classList.add('active');if(m==='auto')applyAutoTheme();else document.body.setAttribute('data-theme',m)}

function setT(m){fetch('/timer?min='+m)}
function send(m){fetch('/cmd?mode='+m)}

function setCustom(){
  let h=parseInt(document.getElementById('mac-hr').value)||0;
  let m=parseInt(document.getElementById('mac-min').value)||0;
  let t=(h*60)+m;if(t>1440)t=1440;
  if(t>0){setT(t);document.getElementById('mac-hr').value="00";document.getElementById('mac-min').value="00"}
}

function toggleVacation(){fetch('/vacation_toggle')}
function saveCal(){
  let s=dStart.value.replace(/-/g,''),e=dEnd.value.replace(/-/g,'');
  if(!s)s="0";if(!e)e="0";
  fetch('/schedule?s='+s+'&e='+e).then(()=>{
    let b=document.getElementById('btn-cal');b.innerText="Saved";b.classList.add('saved');
    setTimeout(()=>{b.innerText="Set Schedule";b.classList.remove('saved')},2000);
  });
}

function update(){
  fetch('/status').then(r=>r.json()).then(d=>{
    const hero=document.getElementById('hero'),temp=document.getElementById('temp'),st=document.getElementById('st'),td=document.getElementById('td'),rw=document.getElementById('real-weather');
    const btnVac=document.getElementById('btn-vacation'),vacLbl=document.getElementById('vac-label'),vacSub=document.getElementById('vac-sub');
    const tileCold=document.getElementById('tile-cold'),tileWarm=document.getElementById('tile-warm'),tileOff=document.getElementById('tile-off'),tileTmr=document.getElementById('tile-timer'),tmrLbl=document.getElementById('timer-tile-label');
    const isOff=d.status.includes("Standby")||d.status.includes("Vacation");

    temp.innerText=isOff?"OFF":d.temp+"\u00B0";
    temp.style.opacity=isOff?"0.3":"1";
    // Fix 15: append clock-sync warning when NTP not yet acquired
    st.innerText=d.ntpOk?d.status:(d.status+" ⚠️ Clock syncing");
    if(isOff){hero.setAttribute('data-state','off');hero.classList.remove('active')}
    else if(d.temp<20){hero.setAttribute('data-state','cold');hero.classList.add('active')}
    else{hero.setAttribute('data-state','warm');hero.classList.add('active')}
    document.getElementById('hero-icon').textContent=(!isOff&&d.temp>=20)?"\u2600\uFE0F":"\u2744";

    if(d.bmeOk){rw.textContent=d.rTemp.toFixed(1)+"\u00B0 \u00B7 "+d.rHum.toFixed(0)+"% humidity";rw.style.display="block"}

    if(d.timerSecs>0){td.style.display="block";let m=Math.floor(d.timerSecs/60),s=d.timerSecs%60;td.innerText=(m<10?"0"+m:m)+":"+(s<10?"0"+s:s)}
    else{td.style.display="none"}

    tileCold.classList.toggle('active',!isOff&&d.temp<20);
    tileWarm.classList.toggle('active',!isOff&&d.temp>=20);
    tileOff.classList.toggle('active',isOff&&!d.status.includes("Vacation"));

    if(d.timerSecs>0){let m=Math.floor(d.timerSecs/60),s=d.timerSecs%60;tmrLbl.textContent=(m<10?"0"+m:m)+":"+(s<10?"0"+s:s)+" remaining";tileTmr.classList.add('active')}
    else{tmrLbl.textContent="No Timer";tileTmr.classList.remove('active')}

    document.querySelectorAll('.preset').forEach(b=>b.classList.remove('active'));
    let customBtn=document.getElementById('tm-custom');
    if(d.activeMins>0){
      let b=document.getElementById('tm'+d.activeMins);
      if(b){b.classList.add('active');customBtn.innerText="Start Custom Timer"}
      else{let dH=Math.floor(d.activeMins/60),dM=d.activeMins%60,txt="Running: ";if(dH>0)txt+=dH+"h ";if(dM>0||dH===0)txt+=dM+"m";customBtn.innerText=txt.trim()}
    }else{customBtn.innerText="Start Custom Timer"}

    for(let i=1;i<=2;i++){let sl=document.getElementById("slot"+i);if(sl)sl.className=(d.slot==i)?"sched-row active":"sched-row"}

    if(document.activeElement!==dStart&&d.vacS>0)dStart.value=d.vacS.toString().replace(/(\d{4})(\d{2})(\d{2})/,'$1-$2-$3');
    if(document.activeElement!==dEnd&&d.vacE>0)dEnd.value=d.vacE.toString().replace(/(\d{4})(\d{2})(\d{2})/,'$1-$2-$3');
    if(d.vacS===0)dStart.value="";
    if(d.vacE===0)dEnd.value="";

    if(d.vacation){btnVac.classList.add('on');vacLbl.innerText=d.manVac?"Manual Vacation":"Scheduled";vacSub.innerText="Schedule paused"}
    else{btnVac.classList.remove('on');vacLbl.innerText="Off";vacSub.innerText="Tap to pause schedule"}
  });
  if(currentMode==='auto')applyAutoTheme();
}
setInterval(update,1000);
</script>
</body></html>
)rawliteral";

// =============================================================================
// Internal Vacation State Evaluation
// =============================================================================
void evaluateVacationState() {
  struct tm ti;
  if (!getLocalTime(&ti, 50))
    return;

  long todayInt =
      (ti.tm_year + 1900) * 10000 + (ti.tm_mon + 1) * 100 + ti.tm_mday;
  bool scheduleActive =
      (vacationStartDate > 0 && vacationEndDate > 0 &&
       todayInt >= vacationStartDate && todayInt <= vacationEndDate);

  vacationMode = (manualVacation || scheduleActive);
}

void clearAllVacationModes() {
  manualVacation = false;
  vacationStartDate = 0;
  vacationEndDate = 0;
  vacationMode = false;

  preferences.putBool("manVac", false);
  preferences.putLong("vacS", 0);
  preferences.putLong("vacE", 0);
}

// =============================================================================
// Reference: IR Operational Commands
// =============================================================================
void turnAcOFF() {
  // Prepare the IR object (no shared state touched yet)
  ac.off();

  // Blast IR outside the lock — blocks ~1s, must not hold mutex
  for (int i = 0; i < 5; i++) {
    ac.send();
    delay(200);
  }

  // Update shared state atomically
  if (xSemaphoreTake(stateMux, portMAX_DELAY)) {
    if (vacationMode)
      strncpy(systemStatus, "Vacation Mode", sizeof(systemStatus));
    else
      strncpy(systemStatus, "Standby", sizeof(systemStatus));
    timerActive = false;
    activeTimerMinutes = 0;
    xSemaphoreGive(stateMux);
  }
}

void turnAcON(int temp) {
  // Configure IR object (stateless, safe outside lock)
  ac.on();
  ac.setTemp(temp);
  ac.setFan(kDaikinFanAuto);
  ac.setMode(kDaikinCool);
  ac.setSwingVertical(false);

  // Blast IR outside the lock — blocks ~1s, must not hold mutex
  for (int i = 0; i < 5; i++) {
    ac.send();
    delay(200);
  }

  // Update shared state atomically
  if (xSemaphoreTake(stateMux, portMAX_DELAY)) {
    currentTemp = temp;
    strncpy(systemStatus, (temp < 20) ? "Max Cool" : "Comfort",
            sizeof(systemStatus));
    xSemaphoreGive(stateMux);
  }
}

// =============================================================================
// Setup
// =============================================================================
void setup() {
  Serial.begin(115200);
  ac.begin();
  stateMux = xSemaphoreCreateMutex(); // Must be created before any task use

  preferences.begin("ac-prefs", false);
  vacationStartDate = preferences.getLong("vacS", 0);
  vacationEndDate = preferences.getLong("vacE", 0);
  manualVacation = preferences.getBool("manVac", false);

  Wire.begin(SDA_PIN, SCL_PIN);
  delay(200);
  if (!bme.begin(0x76, &Wire)) {
    if (!bme.begin(0x77, &Wire))
      bmeActive = false;
    else
      bmeActive = true;
  } else
    bmeActive = true;

  // WiFiManager: auto-connects using saved credentials.
  // If no credentials are saved (or reset), it opens a config AP.
  // IP is assigned via DHCP. Check your router for the device address.
  WiFiManager wm;
  wm.setConfigPortalTimeout(portalTimeout);
  wm.setConnectTimeout(20);
  if (!wm.autoConnect(wifiApName, wifiApPass)) {
    Serial.println("[AC] WiFiManager portal timed out — restarting");
    delay(500);
    ESP.restart();
  }
  Serial.print("[AC] Connected. IP: ");
  Serial.println(WiFi.localIP());

  configTime(gmtOffset_sec, daylightOffset_sec, "pool.ntp.org");
  ArduinoOTA.setHostname(deviceName);
  ArduinoOTA.setPassword("admin");
  ArduinoOTA.begin();
  server.on("/", []() { server.send(200, "text/html", index_html); });

  // Reset WiFi credentials and reopen the config portal on next reboot.
  server.on("/reset-wifi", []() {
    server.send(200, "text/plain",
                "WiFi credentials cleared. Rebooting into config portal...");
    delay(500);
    WiFiManager wm;
    wm.resetSettings();
    ESP.restart();
  });

  server.on("/cmd", []() {
    String m = server.arg("mode");
    if (m == "off") {
      manualOverride = false; // Explicit off cancels override
      turnAcOFF();
    } else if (m == "on16") {
      if (vacationMode)
        clearAllVacationModes();
      manualOverride = true; // Fix 9-A: user takes control
      turnAcON(16);
    } else if (m == "on25") {
      if (vacationMode)
        clearAllVacationModes();
      manualOverride = true; // Fix 9-A: user takes control
      turnAcON(25);
    }
    server.send(200, "text/plain", "OK");
  });

  server.on("/vacation_toggle", []() {
    evaluateVacationState();

    if (vacationMode) {
      clearAllVacationModes();
      if (strncmp(systemStatus, "Vacation Mode", 13) == 0) {
        strncpy(systemStatus, "Standby", sizeof(systemStatus));
      }
    } else {
      manualVacation = true;
      preferences.putBool("manVac", true);
      evaluateVacationState();
      turnAcOFF();
    }

    server.send(200, "text/plain", "OK");
  });

  server.on("/schedule", []() {
    vacationStartDate = server.arg("s").toInt();
    vacationEndDate = server.arg("e").toInt();
    preferences.putLong("vacS", vacationStartDate);
    preferences.putLong("vacE", vacationEndDate);
    evaluateVacationState();

    if (vacationMode) {
      turnAcOFF();
    } else if (strncmp(systemStatus, "Vacation Mode", 13) == 0) {
      strncpy(systemStatus, "Standby", sizeof(systemStatus));
    }

    server.send(200, "text/plain", "OK");
  });

  server.on("/timer", []() {
    int mins = server.arg("min").toInt();
    if (mins > 1440)
      mins = 1440;

    if (mins > 0) {
      if (vacationMode)
        clearAllVacationModes();
      manualOverride = true; // Fix 9-A: timer is a manual action
      timerStartMillis = millis();
      timerDuration =
          (unsigned long)mins * 60000UL; // Fix 4: unsigned prevents overflow
      timerActive = true;
      activeTimerMinutes = mins;
      turnAcON(16);
    } else {
      manualOverride = false;
      timerActive = false;
      activeTimerMinutes = 0;
      turnAcOFF();
    }
    server.send(200, "text/plain", "OK");
  });

  server.on("/status", []() {
    struct tm ti;
    // Fix 15: capture NTP sync state from return value
    bool ntpOk = getLocalTime(&ti, 50);

    // Fix 11: use helper instead of duplicated hour checks
    currentSlotIndex = ntpOk && isNightWindow(ti) ? 1 : 2;

    // Fix 4: guard remaining-secs to prevent unsigned underflow -> negative
    // long
    long rs = 0;
    if (timerActive) {
      unsigned long elapsed = millis() - timerStartMillis;
      if (elapsed < timerDuration)
        rs = (long)((timerDuration - elapsed) / 1000UL);
    }

    // Fix 13: read BME280 at most once every 30 seconds
    static float cachedRTemp = 0.0, cachedRHum = 0.0;
    static unsigned long lastBmeMs = 0;
    if (bmeActive && (millis() - lastBmeMs >= 30000UL)) {
      cachedRTemp = bme.readTemperature();
      cachedRHum = bme.readHumidity();
      lastBmeMs = millis();
    }

    // Fix 6: enlarged buffer + overflow guard; ntpOk field added (Fix 15)
    char j[660];
    int written = snprintf(
        j, sizeof(j),
        "{\"status\":\"%s\",\"temp\":%d,\"slot\":%d,\"activeMins\":%d,"
        "\"timerSecs\":%ld,\"bmeOk\":%s,\"rTemp\":%.1f,\"rHum\":%.1f,"
        "\"vacation\":%s,\"manVac\":%s,\"vacS\":%ld,\"vacE\":%ld,"
        "\"ntpOk\":%s}",
        systemStatus, currentTemp, currentSlotIndex, activeTimerMinutes, rs,
        bmeActive ? "true" : "false", cachedRTemp, cachedRHum,
        vacationMode ? "true" : "false", manualVacation ? "true" : "false",
        vacationStartDate, vacationEndDate, ntpOk ? "true" : "false");
    if (written < 0 || written >= (int)sizeof(j)) {
      server.send(500, "text/plain", "JSON overflow");
      return;
    }
    server.send(200, "application/json", j);
  });

  server.begin();

  // Fix 10: recover AC state immediately after reboot — don't wait for first
  // loop() tick Wait up to 3s for NTP so we get a valid time right after
  // configTime.
  delay(3000);
  {
    struct tm bt;
    if (getLocalTime(&bt, 500) && isNightWindow(bt) && !vacationMode) {
      int bm = bt.tm_hour * 60 + bt.tm_min;
      if (shouldAcBeOn(bm)) {
        turnAcON(16);
        lastCycleWasOn = true;
      }
      inActiveWindow = true;
    }
  }
}

// =============================================================================
// Explanation: Main Control Loop
// =============================================================================
void loop() {
  ArduinoOTA.handle();
  server.handleClient();

  if (timerActive && (millis() - timerStartMillis >= timerDuration))
    turnAcOFF();

  static unsigned long lc = 0;
  if (millis() - lc >= 1000) {
    lc = millis();
    evaluateVacationState();

    struct tm t;
    if (getLocalTime(&t)) {

      // Fix 11: single helper replaces three copies of the hour check
      bool inWindow = isNightWindow(t);

      if (inWindow) {
        // Comfort Optimized Protocol: $1.30 = 195 mins across 9 hours.
        // 7 asymmetric cycles shaped to Singapore's nocturnal temp curve.
        // Gaps: short at dusk/dawn (30-40m), long at 03:00 nadir (75m).
        // Fix 12: schedule logic now driven by SCHEDULE[] table above.
        int m = (t.tm_hour * 60) + t.tm_min;
        bool shouldBeOn =
            shouldAcBeOn(m); // Fix 11+12: replaces 7-line if-chain

        if (!vacationMode &&
            !manualOverride) { // Fix 9-A: skip if user has override
          if (shouldBeOn && !lastCycleWasOn) {
            turnAcON(16); // Maximize delta T for rapid sensible heat extraction
            if (xSemaphoreTake(stateMux, portMAX_DELAY)) {
              lastCycleWasOn = true;
              xSemaphoreGive(stateMux);
            }
          } else if (!shouldBeOn && lastCycleWasOn) {
            turnAcOFF();
            if (xSemaphoreTake(stateMux, portMAX_DELAY)) {
              lastCycleWasOn = false;
              xSemaphoreGive(stateMux);
            }
          }
        } else if (vacationMode) {
          if (xSemaphoreTake(stateMux, portMAX_DELAY)) {
            lastCycleWasOn = shouldBeOn;
            xSemaphoreGive(stateMux);
          }
        }
        // (manualOverride && !vacationMode): do nothing — user is in control
        inActiveWindow = true;
      } else {
        if (inActiveWindow) {
          turnAcOFF();
          if (xSemaphoreTake(stateMux, portMAX_DELAY)) {
            inActiveWindow = false;
            lastCycleWasOn = false;
            manualOverride =
                false; // Fix 9-A: clear override at 07:00 window end
            xSemaphoreGive(stateMux);
          }
        }
      }
    }
  }
}