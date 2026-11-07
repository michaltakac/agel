"""Run the Agel game client in the native OS over the general byte carrier."""
import argparse
import json
import os
from pathlib import Path
import secrets
import subprocess
import tempfile
import time
import graphical_console as console

parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--challenge', action='store_true')
options=parser.parse_args()
directory=Path('target')/('native-network-'+str(time.time_ns()))
directory.mkdir(mode=0o700)
print('Evidence:', directory, flush=True)
manifest=json.loads(subprocess.run(['python3','scripts/osrs/world_manifest.py'],capture_output=True,text=True,check=True).stdout)
manifest['scope']='native OS login, live observations and movement smoke test; no Exo task score'
(directory/'world.json').write_text(json.dumps(manifest,indent=2)+'\n')
catalog=Path('crates/agel-stdlib/stdlib.agel').read_text()
a=catalog.index('(module agel/sequence\n');b=catalog.find('\n(module ',a+1)
source=catalog[a:b]+'\n'
source+=''.join(Path('crates/agel-stdlib/'+name+'.agel').read_text()+'\n' for name in ['bytes','byte-stream','buffered-stream'])
source+='''(import agel/byte-stream)
(import agel/buffered-stream)
(def stream (buffered-stream (byte-stream byte-request) 256 (list (request-capability "byte/relay" "*"))))
(def wire-open (get stream 'open)) (def wire-close (get stream 'close))
(def wire-read (get stream 'read)) (def wire-write (get stream 'write))
(def entropy-read (get stream 'entropy))
'''
modules=['osrs-wire'] if options.challenge else ['osrs-wire','crypto','osrs-crypto','osrs-login','osrs-packets','osrs-bits','osrs-npcs','osrs-game']
source+=''.join(Path('crates/agel-stdlib/'+name+'.agel').read_text()+'\n' for name in modules)
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
  (dict 'start (get stream 'start-service) 'stop (get stream 'stop-service))))
(console-log "native-network: cipher state initialized inside Agel OS")
'''
    source+='(begin (osrs-login '+json.dumps(account['username'])+' '+json.dumps(account['password'])+' context crcs seed (entropy-read 24)) #t)\n'
    source+='''(console-log "native-network: password login accepted")
(def ready (osrs-await game (fn (g) (if (osrs-stat g 13) (if (get (get g 'inventories) 93) #t #f) #f))))
(if (= (get (osrs-stat ready 13) 'level) 1) #t (signal 'native/test "unexpected initial Smithing level"))
(console-log (text-concat "native-network: observations " (print-form
  (dict 'smithing-level (get (osrs-stat ready 13) 'level) 'coins (osrs-inventory-count ready 93 995)))))
(def walked (osrs-wait-ticks (osrs-walk ready 3223 3218) 3))
(if (< 0 (get walked 'keepalives)) #t (signal 'native/test "keepalive path was not exercised"))
(console-log (text-concat "native-network: keepalives " (print-form (get walked 'keepalives))))
(wire-close)
(console-log "native-network: client completed: login, observations, walking packet and tick consumption inside Agel OS")
#t
'''
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
            result=subprocess.run(['target/release/examples/byte_bridge',str(serial),'127.0.0.1:43594',
                ':exec agel -- --no-stdlib /data/client'],stdout=log,stderr=subprocess.STDOUT,timeout=960)
    finally:
        machine.terminate();machine.wait(timeout=10)
output=(directory/'serial.log').read_text()
if not options.challenge:assert account['password'] not in output,'credential appeared in serial output'
print('\n'.join(line for line in output.splitlines() if 'native-network:' in line or 'agel: error' in line or 'exited with status' in line),flush=True)
marker='native-network: PASS:' if options.challenge else 'native-network: client completed:'
assert result.returncode==0 and marker in output,'native network scenario failed; evidence retained'
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
