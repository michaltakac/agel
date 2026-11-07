"""Run the Agel game client in the native OS over the general byte carrier."""
import argparse
import json
import os
from pathlib import Path
import secrets
import re
from native_provider_support import provider_environment, decode_judge_ledger
import subprocess
import tempfile
import time
import graphical_console as console

parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--challenge', action='store_true')
parser.add_argument('--repair', action='store_true', help='repair a general tool during the same native connection')
parser.add_argument('--optimize', action='store_true', help='optimize the live byte service policy from a recorded observation')
parser.add_argument('--live', action='store_true', help='use real providers for --repair or --optimize')
options=parser.parse_args()
editing=options.repair or options.optimize
if sum([options.challenge,options.repair,options.optimize])>1:parser.error('--challenge, --repair and --optimize are exclusive')
if options.live and not editing:parser.error('--live requires --repair or --optimize')
directory=Path('target')/('native-network-'+str(time.time_ns()))
directory.mkdir(mode=0o700)
print('Evidence:', directory, flush=True)
proposal="(dict 'diagnosis \"fixed cadence pays for more service calls than the observed work bound requires\" 'after '(fn (limit) (* 64 (/ limit 64))) 'predictions '(64 192))" if options.optimize else None
environment=provider_environment(directory, options.live, delay=20, proposal=proposal) if editing else dict(os.environ)
environment['AGEL_BYTE_ENDPOINT']='127.0.0.1:43594'
if editing:print('Providers:', 'real' if options.live else 'fixtures; writer delayed 20 seconds', flush=True)
manifest=json.loads(subprocess.run(['python3','scripts/osrs/world_manifest.py'],capture_output=True,text=True,check=True).stdout)
manifest['scope']='native OS login, live observations, seeded general tool repair and saved movement; no Exo task score' if options.repair else 'native OS login, live observations and movement smoke test; no Exo task score'
if options.optimize:manifest['scope']='native measured byte policy optimization, recorded byte replay and saved movement; no Exo task score'
(directory/'world.json').write_text(json.dumps(manifest,indent=2)+'\n')
catalog=Path('crates/agel-stdlib/stdlib.agel').read_text()
a=catalog.index('(module agel/sequence\n');b=catalog.find('\n(module ',a+1)
source=catalog[a:b]+'\n'
source+=''.join(Path('crates/agel-stdlib/'+name+'.agel').read_text()+'\n' for name in ['bytes','byte-stream','stream-buffer','services','service-stream'])
source+='''(import agel/byte-stream)
(import agel/service-stream)
(def stream (service-stream (byte-stream byte-request) 256))
(def wire-open (get stream 'open)) (def wire-close (get stream 'close))
(def wire-read (get stream 'read)) (def wire-write (get stream 'write))
(def entropy-read (get stream 'entropy))
'''
modules=['osrs-wire'] if options.challenge else ['osrs-wire','crypto','osrs-crypto','osrs-login','osrs-packets','osrs-bits','osrs-npcs','osrs-game']
source+=''.join(Path('crates/agel-stdlib/'+name+'.agel').read_text()+'\n' for name in modules)
if editing:
    for name in ['meta','judgment']:
        a=catalog.index('(module agel/'+name+'\n');b=catalog.find('\n(module ',a+1)
        source+=catalog[a:b if b>=0 else len(catalog)]+'\n'
    source+='(module agel/native-reader (export native-read) (def native-read '+Path('crates/agel-stdlib/native-reader.agel').read_text()+'))\n'
    source+=''.join(Path('crates/agel-stdlib/'+name+'.agel').read_text()+'\n' for name in ['evolution','judge-log','repair','inference','live-tools','batches'])
    source+=Path('examples/native-session-support.agel').read_text()+'\n'
    if options.optimize:
        source+=Path('crates/agel-stdlib/work-profile.agel').read_text()+'\n'
        source+=Path('examples/native-service-optimize.agel').read_text()+'\n'
    else:
        source+=Path('examples/native-session-repair.agel').read_text()+'\n'
source+='''(import agel/osrs-wire)
(osrs-js5-open 241 '(1 2 3 4))
(def master (osrs-js5-group 255 255))
(wire-close)
(console-log "native-network: cache negotiation and master index received inside Agel OS")
'''
if options.challenge:
    source+='''(osrs-game-open) (wire-close)
(console-log "native-network: PASS: private-world cache and login challenge handled by Agel in the OS") #t
'''
else:
    account={'username':'nat'+secrets.token_hex(4),'password':secrets.token_hex(12)}
    (directory/'account.json').write_text(json.dumps(account));os.chmod(directory/'account.json',0o600)
    key=subprocess.run(['docker','exec','agel-osrs-world','cat','/world/.data/client.key'],capture_output=True,text=True,check=True).stdout
    assert key.split('Exponent: ')[1].splitlines()[0].strip()=='10001'
    modulus=key.split('Modulus: ')[1].strip()
    source+='''(import agel/osrs-crypto) (import agel/osrs-login) (import agel/osrs-game)
(def crcs (osrs-cache-crcs master))
'''
    source+='(def context (rsa-context '+json.dumps(modulus)+'))\n'
    source+='''(console-log "native-network: RSA public context computed inside Agel OS")
(def seed (osrs-random-seed))
(def game (assoc (osrs-game-state seed) 'maintenance
  (dict 'start (get stream 'start-service) 'stop (get stream 'stop-service) 'pulse (get stream 'pulse))))
(console-log "native-network: cipher state initialized inside Agel OS")
'''
    if options.optimize:
        source+="(def game (assoc game 'maintenance (assoc (get game 'maintenance) 'quantum retained-quantum)))\n"
    source+='(begin (osrs-login '+json.dumps(account['username'])+' '+json.dumps(account['password'])+' context crcs seed (entropy-read 24)) #t)\n'
    source+='''(console-log "native-network: password login accepted")
(def ready (osrs-await game (fn (g) (if (osrs-stat g 13) (if (get (get g 'inventories) 93) #t #f) #f))))
(if (= (get (osrs-stat ready 13) 'level) 1) #t (signal 'native/test "unexpected initial Smithing level"))
(console-log (text-concat "native-network: observations " (print-form
  (dict 'smithing-level (get (osrs-stat ready 13) 'level) 'coins (osrs-inventory-count ready 93 995)))))
'''
    if editing:
        source+="(def prior-services (get ready 'keepalives))\n(osrs-service-start ready)\n"
        if options.optimize:
            source+="(def recorded-field (largest-field (get ready 'history) 'bytes))\n"
            source+=Path('examples/native-service-optimize-run.agel').read_text()+'\n'
            source+='(def policy-uses 0)\n'
        else:
            source+=Path('examples/native-session-repair-run.agel').read_text()+'\n'
        source+='(def ready (osrs-service-stop ready))\n'
        label='native-opt: optimization-stage service writes ' if options.optimize else 'native-session: model-wait service writes '
        source+="(console-log (text-concat "+json.dumps(label)+" (print-form (- (get ready 'keepalives) prior-services))))\n"
    if options.repair:
        source+='''(import agel/batches)
(def execution (run-batches retained-plan (fn (state x) (osrs-walk state x 3218)) ready '(3221 3221 3221 3222 3223) 3))
(check (= (count (get execution 'remaining)) 0) "planner left game actions unexecuted")
(console-log (text-concat "native-session: executed batches " (print-form (get execution 'completed))))
(def walked (osrs-wait-ticks (get execution 'state) 3))
'''
    else:
        source+='(def walked (osrs-wait-ticks (osrs-walk ready 3223 3218) 3))\n'
    if options.optimize:
        source+='''(check (< 0 policy-uses) "no live packet used the changed cadence")
(console-log (text-concat "native-opt: live uses " (print-form policy-uses)))
'''
    source+='''
(if (< 0 (get walked 'keepalives)) #t (signal 'native/test "keepalive path was not exercised"))
(console-log (text-concat "native-network: keepalives " (print-form (get walked 'keepalives))))
(wire-close)
'''
    if options.optimize:
        source+='''(check (= (text-slice (file-read "judge-log") tape-offset (+ tape-offset (text-bytes replay-tape))) replay-tape)
  "recorded workload differed in protected storage")
(console-log "native-opt: recorded workload verified in append-only log")
'''
    source+='''
(console-log "native-network: client completed: login, observations, walking packet and tick consumption inside Agel OS")
#t
'''
source='(begin\n'+source+'\n#t)\n'
script=directory/'client.agel';script.write_text(source);os.chmod(script,0o600)
disk=console.prepared_image('target/boot/agel-v1.img',directory,blank_sectors=1024)
for cmd in [['python3','scripts/install-program.py',disk,'agel','boot/posix/target/x86_64-unknown-none/release/agel'],
            ['python3','scripts/install-program.py','--region','data',disk,'client',str(script)]]:
    subprocess.run(cmd,check=True,capture_output=True)
run_started=time.monotonic()
with tempfile.TemporaryDirectory(prefix='agel-native-network-',dir='/tmp') as sockets:
    serial=Path(sockets)/'serial'
    machine=subprocess.Popen(['qemu-system-x86_64','-machine','pc,accel=tcg','-m','64M','-display','none','-no-reboot','-vga','std',
        '-serial',f'unix:{serial},server=on,wait=on','-drive',f'format=raw,file={Path(disk).resolve()},if=ide,index=0,media=disk'],
        stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    try:
        deadline=time.monotonic()+15
        while not serial.exists():
            if machine.poll() is not None or time.monotonic()>deadline:raise RuntimeError('serial unavailable')
            time.sleep(.05)
        with (directory/'serial.log').open('w') as log:
            command=['target/release/examples/guest_inference',str(serial),str(directory.resolve()),'process agel exited with status',':fs-format',':exec agel -- --no-stdlib /data/client'] if editing else ['target/release/examples/byte_bridge',str(serial),'127.0.0.1:43594',':exec agel -- --no-stdlib /data/client']
            result=subprocess.run(command,stdout=log,stderr=subprocess.STDOUT,env=environment,timeout=960)
    finally:
        machine.terminate();machine.wait(timeout=10)
output=(directory/'serial.log').read_text()
if not options.challenge:assert account['password'] not in output,'credential appeared in serial output'
print('\n'.join(line for line in output.splitlines() if 'native-network:' in line or 'native-session:' in line or 'native-opt:' in line or 'inference-bridge:' in line or 'agel: error' in line or 'exited with status' in line),flush=True)
marker='native-network: PASS:' if options.challenge else 'native-network: client completed:'
passed=result.returncode==0 and marker in output and 'process agel exited with status 0' in output
if editing:
    decode_judge_ledger(disk,directory,environment['TYPESAFEAI_API_KEY'],options.live,required=result.returncode==0 and 'process agel exited with status 0' in output)
    assert passed, 'native session failed; disk, serial log and judge records retained'
    if options.optimize:
        assert 'native-opt: policy committed after text judgment and measured replay' in output
        assert re.search(r'native-opt: live uses [1-9][0-9]*',output)
        assert 'native-opt: recorded workload verified in append-only log' in output
    else:
        assert 'native-session: tool repaired:' in output, 'live tool repair did not complete'
    assert output.count('operation open ') == 2, 'game session was reopened during repair'
    assert output.count('operation close ') == 2, 'live byte session was not closed cleanly'
    assert environment['TYPESAFEAI_API_KEY'] not in output, 'provider credential appeared in serial'
    starts={job:(route,int(at)) for job,route,at in re.findall(r'inference-bridge: started job (\d+) provider (\w+) at-ms (\d+)',output)}
    durations=[int(at)-starts[job][1] for job,at in re.findall(r'inference-bridge: completed job (\d+) at-ms (\d+) status ok',output) if job in starts and starts[job][0]=='codex']
    assert durations, 'no successful writer job was observed'
    if not options.live:assert max(durations)>=20000, 'delayed-provider coverage was not exercised'

assert passed,'native network scenario failed; evidence retained'
if not options.challenge:
    compiler=Path('/opt/homebrew/opt/openjdk@17/bin/javac')
    build=directory/'oracle';build.mkdir()
    subprocess.run([str(compiler) if compiler.is_file() else 'javac','-d',str(build),'scripts/osrs/AccountState.java'],check=True,capture_output=True)
    subprocess.run(['docker','cp',str(build/'AccountState.class'),'agel-osrs-world:/tmp/AccountState.class'],check=True,capture_output=True)
    deadline=time.monotonic()+45
    while True:
        saved=subprocess.run(['docker','exec','agel-osrs-world','sh','-c',
            'exec "$JAVA_HOME/bin/java" -cp "/tmp:/world/app/lib/*" AccountState "$1"','probe',account['username']],capture_output=True,text=True)
        if saved.returncode==0 and 'x=3223 z=3218 level=0' in saved.stdout:break
        if time.monotonic()>deadline:raise AssertionError('walking state was not saved at expected coordinates')
        time.sleep(1)
    assert 'rights= ' in saved.stdout and 'xp_rate_in_hundreds=100' in saved.stdout,saved.stdout
    (directory/'saved-account.txt').write_text(saved.stdout)
    print('Persisted native client:',saved.stdout.splitlines()[0],flush=True)
    print('PASS: native client movement saved with normal rights and 1x XP',flush=True)

print('Elapsed seconds:',round(time.monotonic()-run_started,3),flush=True)
