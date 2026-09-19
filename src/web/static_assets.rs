//! Embedded static web dashboard HTML/CSS/JS loaded from assets/index.html and setup pages.

pub const INDEX_HTML: &str = include_str!("../../assets/index.html");

pub const WIFI_SETUP_HTML: &str = r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>AC Scheduler &mdash; Wi-Fi Setup</title>
<style>
body{font-family:-apple-system,BlinkMacSystemFont,sans-serif;background:#0d1117;color:#c9d1d9;display:flex;justify-content:center;align-items:center;min-height:100vh;margin:0;padding:20px;box-sizing:border-box}
.card{background:#161b22;padding:28px;border-radius:14px;border:1px solid #30363d;width:100%;max-width:380px;box-shadow:0 8px 24px rgba(0,0,0,0.5)}
h2{margin:0 0 8px;color:#58a6ff;font-size:22px}
p{font-size:14px;color:#8b949e;margin:0 0 20px}
label{display:block;margin:14px 0 6px;font-size:13px;color:#8b949e;font-weight:600}
input{width:100%;box-sizing:border-box;padding:12px;background:#0d1117;border:1px solid #30363d;border-radius:8px;color:#fff;font-size:15px;outline:none}
input:focus{border-color:#58a6ff}
button{width:100%;margin-top:22px;padding:13px;background:#238636;color:#fff;border:none;border-radius:8px;font-size:15px;font-weight:600;cursor:pointer}
button:hover{background:#2ea043}
.note{margin-top:16px;font-size:12px;color:#8b949e;line-height:1.4;text-align:center}
</style>
</head>
<body>
<div class="card">
<h2>AC Scheduler Setup</h2>
<p>Connect this device to your local Wi-Fi:</p>
<form action="/wifi_save" method="GET">
<label for="s">Wi-Fi Network (SSID)</label>
<input type="text" id="s" name="s" required placeholder="Network Name" autofocus>
<label for="p">Password</label>
<input type="password" id="p" name="p" placeholder="Password (leave empty if open)">
<button type="submit">Save &amp; Connect</button>
</form>
<div class="note">Settings are stored in NVS flash. The ESP32 will reboot and join your network.</div>
</div>
</body>
</html>"#;

pub const OTA_UPDATE_HTML: &str = r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>AC Scheduler &mdash; OTA Firmware Update</title>
<style>
body{font-family:-apple-system,BlinkMacSystemFont,sans-serif;background:#0d1117;color:#c9d1d9;display:flex;justify-content:center;align-items:center;min-height:100vh;margin:0;padding:20px;box-sizing:border-box}
.card{background:#161b22;padding:28px;border-radius:14px;border:1px solid #30363d;width:100%;max-width:400px;box-shadow:0 8px 24px rgba(0,0,0,0.5)}
h2{margin:0 0 8px;color:#58a6ff;font-size:22px}
p{font-size:14px;color:#8b949e;margin:0 0 20px}
input[type=file]{width:100%;box-sizing:border-box;padding:10px;background:#0d1117;border:1px solid #30363d;border-radius:8px;color:#8b949e;font-size:14px}
button{width:100%;margin-top:20px;padding:13px;background:#1f6feb;color:#fff;border:none;border-radius:8px;font-size:15px;font-weight:600;cursor:pointer}
button:hover{background:#388bfd}
button:disabled{background:#30363d;color:#8b949e;cursor:not-allowed}
#prog{display:none;margin-top:16px;background:#21262d;border-radius:6px;overflow:hidden;height:12px}
#bar{width:0%;height:100%;background:#238636;transition:width .2s}
#msg{margin-top:14px;font-size:13px;text-align:center}
</style>
</head>
<body>
<div class="card">
<h2>Firmware Update</h2>
<p>Upload a new compiled <code>ac-scheduler.bin</code> file:</p>
<input type="file" id="fw" accept=".bin">
<button id="btn" onclick="startOta()">Upload Firmware</button>
<div id="prog"><div id="bar"></div></div>
<div id="msg"></div>
</div>
<script>
function startOta(){
  const f=document.getElementById('fw').files[0];
  if(!f){alert('Please select a .bin firmware file');return}
  const btn=document.getElementById('btn'),prog=document.getElementById('prog'),bar=document.getElementById('bar'),msg=document.getElementById('msg');
  btn.disabled=true;prog.style.display='block';msg.innerText='Uploading firmware...';msg.style.color='#8b949e';
  const xhr=new XMLHttpRequest();
  xhr.open('POST','/update',true);
  xhr.upload.onprogress=e=>{if(e.lengthComputable){const p=Math.round((e.loaded/e.total)*100);bar.style.width=p+'%';msg.innerText='Uploading: '+p+'%'}};
  xhr.onload=()=>{
    if(xhr.status===200){
      bar.style.width='100%';bar.style.background='#238636';
      msg.innerText='Update successful! Device is rebooting...';msg.style.color='#3fb950';
      setTimeout(()=>{window.location.href='/'},5000);
    }else{
      btn.disabled=false;bar.style.background='#da3633';
      msg.innerText='Update failed: '+(xhr.responseText||xhr.statusText);msg.style.color='#f85149';
    }
  };
  xhr.onerror=()=>{btn.disabled=false;bar.style.background='#da3633';msg.innerText='Upload network error';msg.style.color='#f85149'};
  xhr.send(f);
}
</script>
</body>
</html>"#;
