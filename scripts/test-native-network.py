"""Run the Agel game client in the native OS over the general byte carrier."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import secrets
import shutil
import re
from native_provider_support import (
    decode_judge_ledger, judge_frames, judge_receipts_for_trace, provider_environment, verified_judge_ledger,
)
import subprocess
import sys
import tempfile
import time
import graphical_console as console

sys.path.insert(0, str(Path(__file__).with_name("osrs")))
from test_support import combat_xp, combat_xp_saved, melee_failure_saved, melee_levels_saved, melee_xp

parser=argparse.ArgumentParser(description=__doc__)
scenario=parser.add_mutually_exclusive_group()
scenario.add_argument('--melee-xp', type=int, help='train three melee skills inside Agel OS')
scenario.add_argument('--economy', action='store_true', help='mine, smelt and sell a bar inside Agel OS')
scenario.add_argument('--quest', action='store_true', help="gather ingredients and complete Cook's Assistant inside Agel OS")
scenario.add_argument('--challenge', action='store_true')
scenario.add_argument('--craft', action='store_true', help='forge staged adamant stock; use test-osrs-crafting.sh --native')
parser.add_argument('--combat', action='store_true', help='kill one rat on a fresh normal account inside Agel OS')
scenario.add_argument('--repair', action='store_true', help='repair a general tool during the same native connection')
parser.add_argument('--optimize', action='store_true', help='optimize the live byte service policy from a recorded observation')
parser.add_argument('--transfer-from', type=Path, help='resume a verified native combat policy on a fresh account from a donor evidence directory')
parser.add_argument('--live', action='store_true', help='use real providers for --repair or --optimize')
parser.add_argument('--quest-resume', type=Path, help='isolate quest hand-in on a previously gathered account')
options=parser.parse_args()
if options.quest_resume and not options.quest:
    parser.error('--quest-resume requires --quest')
melee=options.melee_xp
if melee is not None and (not 1 <= melee <= 512 or options.combat):
    parser.error('--melee-xp requires a goal of 1..512 and is separate from --combat')
if options.optimize and (options.challenge or options.repair or options.craft or options.economy or options.quest):
    parser.error('--optimize can be combined with --combat or --melee-xp')
editing=options.repair or options.optimize
combat_workload=options.combat or melee is not None
shared_carrier=editing or combat_workload
if options.combat and (options.challenge or options.repair or options.craft or options.economy or options.quest):parser.error('--combat can only be combined with --optimize')
if options.transfer_from and (not combat_workload or editing):
    parser.error('--transfer-from requires --combat or --melee-xp without editing')
if options.live and not editing:parser.error('--live requires --repair or --optimize')
directory=Path('target')/('native-network-'+str(time.time_ns()))
directory.mkdir(mode=0o700)
print('Evidence:', directory, flush=True)
proposal="(dict 'diagnosis \"fixed cadence pays for more service calls than the observed work bound requires\" 'after '(fn (limit) (* 64 (/ limit 64))) 'predictions '(64 192))" if options.optimize else None
environment=provider_environment(directory, options.live, delay=20, proposal=proposal) if shared_carrier else dict(os.environ)
environment['AGEL_BYTE_ENDPOINT']='127.0.0.1:43594'
if melee is not None:
    environment['AGEL_GUEST_TIMEOUT_SECONDS']='1680'
if editing:print('Providers:', 'real' if options.live else 'fixtures; writer delayed 20 seconds', flush=True)
manifest=json.loads(subprocess.run(['python3','scripts/osrs/world_manifest.py'],capture_output=True,text=True,check=True).stdout)
manifest['scope']='native OS login, live observations, seeded general tool repair and saved movement; no Exo task score' if options.repair else 'native OS login, live observations and movement smoke test; no Exo task score'
if options.optimize:
    manifest['scope']='native measured byte policy optimization, recorded byte replay and saved movement; no Exo task score'
    manifest['evaluation_fuel']=250_000_000
    manifest['guest_memory_mib']=1024
if options.combat:
    manifest['scope']='native one-rat combat on a fresh normal account'
    manifest['encoder_policy']='adaptive' if options.optimize else 'fixed'
    manifest['guest_memory_mib']=1024
    manifest['evaluation_fuel']=250_000_000

if melee is not None:
    manifest['scope']='native three-skill melee training on a fresh normal account'
    manifest['goal_xp']=melee
    manifest['encoder_policy']='adaptive' if options.optimize else 'fixed'
    manifest['evaluation_fuel']=250_000_000
    manifest['keepalive_interval_us']=3_000_000
    manifest['guest_memory_mib']=1024

if options.craft:manifest['scope']='native adamant platebody from staged bars on a normal account'
if options.economy:
    manifest['scope']='native first bronze bar sale on a fresh normal account'
    manifest['encoder_policy']='fixed'
    manifest['evaluation_fuel']=250_000_000
    manifest['keepalive_interval_us']=3_000_000
if options.quest:
    manifest['scope']="native Cook's Assistant with gathered ingredients on a fresh normal account"
    manifest['evaluation_fuel']=250_000_000
    manifest['keepalive_interval_us']=3_000_000
    manifest['guest_memory_mib']=1024
    if options.quest_resume:
        prior = options.quest_resume
        original = json.loads((prior/'world.json').read_text())
        if any(original.get(key) != manifest[key] for key in ('image', 'manifest', 'scope')):
            parser.error('quest checkpoint differs from the current world or fresh quest workload')
        trace = (prior/'serial.log').read_text()
        if ('native-quest: milled wheat and collected a pot of flour' not in trace
                or 'process agel exited with status 1' not in trace):
            parser.error('quest checkpoint requires a failed native run that gathered the ingredients')
        manifest['scope']="native Cook's Assistant hand-in on a retained normal account"
        manifest['checkpoint_trace_sha256']=hashlib.sha256(trace.encode()).hexdigest()

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
source += Path('examples/osrs/osrs-wire.agel').read_text() + '\n'
if not options.challenge:
    source += Path('crates/agel-stdlib/crypto.agel').read_text() + '\n'
    modules = ('osrs-crypto', 'osrs-login', 'osrs-packets', 'osrs-bits', 'osrs-npcs', 'osrs-game')
    source += ''.join(Path(f'examples/osrs/{name}.agel').read_text() + '\n' for name in modules)
if combat_workload or options.craft or options.economy or options.quest:
    source+=Path('crates/agel-stdlib/record-chain.agel').read_text()+'\n'
if options.craft:
    source+=Path('examples/osrs/crafting-test.agel').read_text()+'\n'
if options.economy or options.quest:
    economy_policy=Path('examples/osrs/mining-test.agel').read_bytes()
    manifest['economy_policy_sha256']=hashlib.sha256(economy_policy).hexdigest()
    source+=economy_policy.decode()+'\n'
    source+=Path('crates/agel-stdlib/benchmark.agel').read_text()+'\n'
if options.quest:
    quest_policy=Path('examples/osrs/quest-test.agel').read_bytes()
    manifest['quest_policy_sha256']=hashlib.sha256(quest_policy).hexdigest()
    source+=quest_policy.decode()+'\n'
if options.combat or melee is not None:
    combat_module=Path('examples/osrs/combat-test.agel').read_bytes()
    manifest['combat_policy_sha256']=hashlib.sha256(combat_module).hexdigest()
    source+=combat_module.decode()+'\n'
if melee is not None:
    training_module=Path('examples/osrs/combat-training.agel').read_bytes()
    manifest['training_policy_sha256']=hashlib.sha256(training_module).hexdigest()
    source+=training_module.decode()+'\n'
if options.transfer_from:
    donor = options.transfer_from
    previous = json.loads((donor/'world.json').read_text())
    donor_trace = (donor/'serial.log').read_text()
    if not (previous.get('encoder_policy') == 'adaptive'
            and previous.get('goal_xp') == melee
            and 'native-opt: policy committed after text judgment and measured replay' in donor_trace
            and 'native-opt: recorded workload verified in append-only log' in donor_trace
            and re.search(r'^native-opt: live uses [1-9]\d*$', donor_trace, re.M)
            and 'process agel exited with status 0' in donor_trace):
        raise SystemExit('donor must have a verified adaptive combat outcome')
    donor_saved = (donor/'saved-account.txt').read_text()
    if 'rights= ' not in donor_saved or 'xp_rate_in_hundreds=100' not in donor_saved:
        raise SystemExit('donor must have normal rights and 1x XP')
    if melee is not None:
        donor_xp, donor_levels = melee_xp(donor_trace, melee)
        if not melee_levels_saved(donor_levels, donor_saved):
            raise SystemExit('donor saved melee levels or stance differ from the observed outcome')
    else:
        donor_xp = combat_xp(donor_trace)
    if not combat_xp_saved(donor_xp, donor_saved, minimum=melee is not None):
        raise SystemExit('donor saved combat XP differs from the observed outcome')
    donor_ledger, donor_frames = verified_judge_ledger(donor)
    if not judge_receipts_for_trace(donor_ledger, donor_trace):
        raise SystemExit('donor judge journal has no verified receipts')
    for field in ('image', 'manifest', 'combat_policy_sha256'):
        if previous[field] != manifest[field]:
            raise SystemExit('donor differs from current combat contract: '+field)
    donor_account = json.loads((donor/'account.json').read_text())['username']
    with (donor/'disk.img').open('rb') as image:
        donor_hash = hashlib.file_digest(image, 'sha256').hexdigest()
    manifest['scope']+=' with retained policy and workload'
    manifest['encoder_policy']='transferred'
    manifest['donor_disk_sha256']=donor_hash
    manifest['donor_judge_frames_sha256']=hashlib.sha256(donor_frames).hexdigest()

if shared_carrier:
    for name in ['meta','judgment','browse']:
        a=catalog.index('(module agel/'+name+'\n');b=catalog.find('\n(module ',a+1)
        source+=catalog[a:b if b>=0 else len(catalog)]+'\n'
    source+='(module agel/native-reader (export native-read) (def native-read '+Path('crates/agel-stdlib/native-reader.agel').read_text()+'))\n'
    source+=''.join(Path('crates/agel-stdlib/'+name+'.agel').read_text()+'\n' for name in ['evolution','judge-log','repair','inference','research','live-tools','batches'])
    source+=Path('examples/native-session-support.agel').read_text()+'\n'
    if options.optimize or combat_workload:
        source+=Path('crates/agel-stdlib/work-profile.agel').read_text()+'\n'
        if options.transfer_from:
            source+=Path('crates/agel-stdlib/benchmark.agel').read_text()+'\n'
            source+=Path('examples/native-combat-transfer.agel').read_text()+'\n'
        else:
            source+="(def retained-policy nil)\n"
        service_policy=Path('examples/native-service-optimize.agel').read_bytes()
        if melee is not None:
            manifest['service_policy_sha256']=hashlib.sha256(service_policy).hexdigest()
            if options.transfer_from:
                for field in ('service_policy_sha256', 'training_policy_sha256',
                              'evaluation_fuel', 'keepalive_interval_us', 'guest_memory_mib'):
                    if previous.get(field) != manifest[field]:
                        raise SystemExit('donor differs from current melee contract: '+field)
        source+=service_policy.decode()+'\n'
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
    if options.quest_resume:
        account=json.loads((options.quest_resume/'account.json').read_text())
    if options.craft:
        fixture=Path('target/osrs-login-probes/agelcraft.json')
        account={'username':'agelcraft','password':json.loads(fixture.read_text())['password']}
    if options.transfer_from:
        assert account['username'] != donor_account
        source+='(def donor-account '+json.dumps(donor_account)+')\n'
        source+='(def fresh-account '+json.dumps(account['username'])+')\n'
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
    if combat_workload or options.craft or options.economy or options.quest:
        source+="(import agel/record-chain) (def game (assoc game 'history-push record-chain-push))\n"
    if melee is not None or options.economy or options.quest:
        source+=f"""(def game (assoc game 'maintenance
  (assoc (assoc (get game 'maintenance) 'clock-us clock-us)
    'keepalive-interval-us {manifest['keepalive_interval_us']})))
"""
    if options.optimize or combat_workload:
        source+="(def game (assoc game 'maintenance (assoc (get game 'maintenance) 'quantum retained-quantum)))\n"
    source+='(begin (osrs-login '+json.dumps(account['username'])+' '+json.dumps(account['password'])+' context crcs seed (entropy-read 24)) #t)\n'
    source+='(console-log "native-network: password login accepted")\n'
    if not (options.craft or options.economy or options.quest):
        source+='''(def ready (osrs-await game (fn (g) (if (osrs-stat g 13) (if (get (get g 'inventories) 93) #t #f) #f))))
(if (= (get (osrs-stat ready 13) 'level) 1) #t (signal 'native/test "unexpected initial Smithing level"))
(console-log (text-concat "native-network: observations " (print-form
  (dict 'smithing-level (get (osrs-stat ready 13) 'level) 'coins (osrs-inventory-count ready 93 995)))))
'''
    if combat_workload:
        source+='(def task-start (clock-us))\n'
    if options.optimize:
        source+=f"(def continue-after-refusal {'#t' if melee is not None else '#f'})\n"
    if editing and not options.combat:
        source+="(def prior-services (get ready 'keepalives))\n(osrs-service-start ready)\n"
        if options.optimize:
            if melee is not None:
                source+="(def recorded-field (record-chain-fold (largest-field-step 'bytes) nil (get ready 'history)))\n"
            else:
                source+="(def recorded-field (largest-field (get ready 'history) 'bytes))\n"
            source+=Path('examples/native-service-optimize-run.agel').read_text()+'\n'
            source+='(def policy-uses 0)\n'
        else:
            source+=Path('examples/native-session-repair-run.agel').read_text()+'\n'
        source+='(def ready (osrs-service-stop ready))\n'
        label='native-opt: optimization-stage service writes ' if options.optimize else 'native-session: model-wait service writes '
        source+="(console-log (text-concat "+json.dumps(label)+" (print-form (- (get ready 'keepalives) prior-services))))\n"
    if options.combat:
        source+='''(import agel/osrs-combat-test)
(def report-target (fn (target)
  (console-log "native-combat: attack packet acknowledged by the byte carrier")
  (console-log (text-concat "native-combat: target " (print-form target)))))
'''
        if options.optimize:
            source+='''(def combat-after-attack (fn (attacking target)
  (report-target target)
  (osrs-service-start attacking)
  (def recorded-field (record-chain-fold (largest-field-step 'bytes) nil (get attacking 'history)))
'''
            source+=Path('examples/native-service-optimize-run.agel').read_text()+'\n'
            source+="(def policy-uses 0) (osrs-service-stop attacking)))\n"
        else:
            source+="(def combat-after-attack (fn (attacking target) (report-target target) attacking))\n"
        source+='''
(def started (clock-us))
(console-log "native-combat: searching for a rat")
(def combat (test-rat-combat-bounded ready
  (fn (state predicate) (osrs-await-bounded state predicate 320))
  combat-after-attack))
(def finish (clock-us))
(def completed (get combat 'state))
(console-log (text-concat "native-combat: " (print-form (get combat 'report))))
(console-log (text-concat "native-combat: action-us " (print-form (- finish started))))
(console-log (text-concat "native-combat: task-us " (print-form (- finish task-start))))
(def retained-count (record-chain-fold (fn (n _) (+ n 1)) 0 (get completed 'history)))
(console-log (text-concat "native-combat: history-records " (print-form retained-count)))
'''
    elif melee is not None:
        source+=f'''(import agel/osrs-combat-training)
(console-log "native-combat: training three melee skills")
(def log-encounter (fn (skill report)
  (console-log (text-concat "native-combat: encounter " (print-form (list skill report))))))
(def trained (with-handler (* problem)
  (begin
    (console-log (text-concat "native-combat: failure " (print-form problem)))
    (console-log (text-concat "native-combat: task-us " (print-form (- (clock-us) task-start))))
    (signal (get problem 'kind) (get problem 'message) (get problem 'data)))
  (train-rat-melee-result ready {melee} {min(64, (melee + 7) // 8 + 3)} log-encounter)))
(def finish (clock-us))
(def completed (get trained 'state))
(def report (get trained 'report))
(console-log (text-concat "native-combat: " (print-form (dissoc report 'phases))))
(map (fn (phase) (console-log (text-concat "native-combat: " (print-form phase)))) (get report 'phases))
(console-log (text-concat "native-combat: task-us " (print-form (- finish task-start))))
'''
    elif options.quest:
        source+='''(import agel/osrs-quest-test)
(def log-quest (fn (phase) (console-log (text-concat "native-quest: " phase))))
(def run-quest
  (fn (scenario)
    (with-handler (* problem)
      (begin
        (log-quest (print-form problem))
        (signal (get problem 'kind) (get problem 'message) (get problem 'data)))
      (scenario game log-quest))))
'''
        if options.quest_resume:
            source+='''(def quest-trace-keys
  (dict 'talk #t 'resume-dialogue #t
        "IF_OPENSUB" #t "IF_CLOSESUB" #t "IF_RESYNC_V2" #t "IF_SETEVENTS_V2" #t "IF_SETTEXT" #t))
(def trace-quest-record
  (fn (history record)
    (if (get quest-trace-keys (if (get record 'name) (get record 'name) (get record 'action)))
      (let ((data (get (get record 'bytes) 'data)))
        (log-quest (text-concat "dialogue trace "
          (print-form (if (if data (< 512 (text-bytes data)) #f)
                        (assoc record 'bytes (text-slice data 0 512)) record)))))
      nil)
    (record-chain-push history record)))
(def game (assoc game 'history-push trace-quest-record))
(log-quest "verifying retained ingredients and attempting hand-in")
(def quest-report (assoc (run-quest finish-cooks-assistant-using) 'source 'checkpoint))
(console-log (text-concat "native-quest: " (print-form quest-report)))
'''
        else:
            source+='''(import agel/benchmark)
(def started (clock-us))
(log-quest "gathering Cook's Assistant ingredients")
(def quest-report (run-quest gather-and-finish-cooks-assistant-using))
(def finished (clock-us))
(console-log (text-concat "native-quest: " (print-form quest-report)))
(console-log (text-concat "native-quest: score " (print-form
  (benchmark-run (dict 'correct (get quest-report 'passed) 'elapsed-us (- finished started)
    'billing-complete #t 'charges nil)))))
'''
    elif options.economy:
        source+='''(import agel/osrs-mining-test) (import agel/benchmark)
(def started (clock-us))
(console-log "native-economy: earning first coins through mining, smelting and sale")
(def sale (first-bronze-sale-using game
  (fn (phase) (console-log (text-concat "native-economy: " phase)))))
(def finished (clock-us))
(def completed (get sale 'state))
(def sale-report (get sale 'report))
(console-log (text-concat "native-economy: " (print-form sale-report)))
(console-log (text-concat "native-economy: score " (print-form
  (benchmark-run (dict 'correct (get sale-report 'passed) 'elapsed-us (- finished started)
    'coins-before (get sale-report 'coins-before) 'coins-after (get sale-report 'coins-after)
    'billing-complete #t 'charges nil)))))
'''
    elif options.craft:
        source+='''(import agel/osrs-crafting-test)
(def started (clock-us))
(def crafted (forge-adamant-platebody-using game
  (fn (state predicate) (osrs-await-bounded state predicate 320))
  (fn (phase) (console-log (text-concat "native-craft: " phase)))))
(def completed (get crafted 'state))
(console-log (text-concat "native-craft: " (print-form (get crafted 'report))))
(console-log (text-concat "native-craft: task-us " (print-form (- (clock-us) started))))
'''
    elif options.repair:
        source+='''(import agel/batches)
(def execution (run-batches retained-plan (fn (state x) (osrs-walk state x 3218)) ready '(3221 3221 3221 3222 3223) 3))
(check (= (count (get execution 'remaining)) 0) "planner left game actions unexecuted")
(console-log (text-concat "native-session: executed batches " (print-form (get execution 'completed))))
(def completed (osrs-wait-ticks (get execution 'state) 3))
'''
    else:
        source+='(def completed (osrs-wait-ticks (osrs-walk ready 3223 3218) 3))\n'
    if options.transfer_from:
        if melee is None:
            source+='''(check (= (get (get combat 'report) 'attack-before) 0) "transfer account has prior combat XP")
(check (= (get (get combat 'report) 'hitpoints-before) 1154) "transfer account has prior Hitpoints XP")
'''
        report="report" if melee is not None else "(get combat 'report)"
        source+=f'''(check (< 0 policy-uses) "retained policy did not serve this connection")
(check (= policy-versions retained-policy) "retained policy changed during transfer")
(check (= donor-audit (file-read "judge-log")) "retained audit log changed during transfer")
(def transfer-score
  (benchmark-transfer (dict 'account donor-account 'retained-state retained-lineage)
    (dict 'account fresh-account 'fresh-account #t 'initial-state retained-lineage
      'correct (get {report} 'passed) 'elapsed-us (- finish task-start)
      'billing-complete #t 'charges nil)))
(console-log (text-concat "native-transfer: score " (print-form transfer-score)))
(console-log (text-concat "native-transfer: retained policy uses " (print-form policy-uses)))
'''
    if options.optimize:
        source+='''(if policy-activated
  (check (< 0 policy-uses) "no live packet used the changed cadence")
  (check (= policy-uses 0) "refused policy served live packets"))
(console-log (text-concat "native-opt: live uses " (print-form policy-uses)))
'''
    if not options.quest:
        source+='''
(if (< 0 (get completed 'keepalives)) #t (signal 'native/test "keepalive path was not exercised"))
(console-log (text-concat "native-network: keepalives " (print-form (get completed 'keepalives))))
'''
    source+='(wire-close)\n'
    if options.optimize:
        source+='''(check (= (text-slice (file-read "judge-log") tape-offset (+ tape-offset (text-bytes replay-tape))) replay-tape)
  "recorded workload differed in protected storage")
(console-log "native-opt: recorded workload verified in append-only log")
'''
    completion = 'login, observations and rat combat' if options.combat else 'login, observations, walking packet and tick consumption'
    if melee is not None:completion='login, observations and melee training'
    if options.craft:completion='login, observations and adamant platebody'
    if options.economy:completion='login, mining, smelting and first bronze sale'
    if options.quest:completion="login and Cook's Assistant hand-in" if options.quest_resume else "login, gathered ingredients and Cook's Assistant"
    source+='(console-log "native-network: client completed: '+completion+' inside Agel OS")\n#t\n'
(directory/'world.json').write_text(json.dumps(manifest,indent=2)+'\n')
source='(begin\n'+source+'\n#t)\n'
script=directory/'client.agel';script.write_text(source);os.chmod(script,0o600)
if options.transfer_from:
    disk=str(directory/'disk.img')
    shutil.copyfile(donor/'disk.img',disk)
    with open(disk,'rb') as image:
        assert hashlib.file_digest(image,'sha256').hexdigest() == donor_hash
else:
    disk=console.prepared_image('target/boot/agel-v1.img',directory,blank_sectors=1024)
for cmd in [['python3','scripts/install-program.py',disk,'agel','boot/posix/target/x86_64-unknown-none/release/agel'],
            ['python3','scripts/install-program.py','--region','data',disk,'client',str(script)]]:
    subprocess.run(cmd,check=True,capture_output=True)
run_started=time.monotonic()
with tempfile.TemporaryDirectory(prefix='agel-native-network-',dir='/tmp') as sockets:
    serial=Path(sockets)/'serial'
    guest_memory=f"{manifest.get('guest_memory_mib',64)}M"
    machine=subprocess.Popen(['qemu-system-x86_64','-machine','pc,accel=tcg','-m',guest_memory,'-display','none','-no-reboot','-vga','std',
        '-serial',f'unix:{serial},server=on,wait=on','-drive',f'format=raw,file={Path(disk).resolve()},if=ide,index=0,media=disk'],
        stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    try:
        deadline=time.monotonic()+15
        while not serial.exists():
            if machine.poll() is not None or time.monotonic()>deadline:raise RuntimeError('serial unavailable')
            time.sleep(.05)
        with (directory/'serial.log').open('w') as log:
            setup=[] if options.transfer_from else [':fs-format']
            guest_command=':exec agel -- --no-stdlib /data/client'
            if 'evaluation_fuel' in manifest:
                guest_command=f":exec agel -- --no-stdlib --fuel {manifest['evaluation_fuel']} /data/client"
            command=['target/release/examples/guest_inference',str(serial),str(directory.resolve()),'process agel exited with status',*setup,guest_command] if shared_carrier else ['target/release/examples/byte_bridge',str(serial),'127.0.0.1:43594',guest_command]
            result=subprocess.run(command,stdout=log,stderr=subprocess.STDOUT,env=environment,timeout=1800 if melee is not None or options.quest else 960)
    finally:
        machine.terminate();machine.wait(timeout=10)
output=(directory/'serial.log').read_text()
if not options.challenge:assert account['password'] not in output,'credential appeared in serial output'
print('\n'.join(line for line in output.splitlines() if 'native-network:' in line or 'native-session:' in line or 'native-opt:' in line or 'native-combat:' in line or 'native-transfer:' in line or 'native-craft:' in line or 'native-economy:' in line or 'native-quest:' in line or 'inference-bridge:' in line or 'agel:' in line or 'exited with status' in line),flush=True)
marker='native-network: PASS:' if options.challenge else 'native-network: client completed:'
passed=result.returncode==0 and marker in output and 'process agel exited with status 0' in output


def capture_account(predicate, varps):
    compiler=Path('/opt/homebrew/opt/openjdk@17/bin/javac')
    build=directory/'oracle';build.mkdir()
    subprocess.run([str(compiler) if compiler.is_file() else 'javac','-d',str(build),'scripts/osrs/AccountState.java'],check=True,capture_output=True)
    subprocess.run(['docker','cp',str(build/'AccountState.class'),'agel-osrs-world:/tmp/AccountState.class'],check=True,capture_output=True)
    deadline=time.monotonic()+45
    while True:
        saved=subprocess.run(['docker','exec','agel-osrs-world','sh','-c',
            'exec "$JAVA_HOME/bin/java" -cp "/tmp:/world/app/lib/*" AccountState "$@"','probe',account['username'],
            *varps],capture_output=True,text=True)
        if saved.returncode==0 and predicate(saved.stdout):
            (directory/'saved-account.txt').write_text(saved.stdout)
            return saved.stdout
        if time.monotonic()>deadline:raise AssertionError('expected state was not saved: '+saved.stdout+saved.stderr)
        time.sleep(1)


if editing:
    judge_dispatched=bool(re.search(r'^inference-bridge: started job \d+ provider jev ',output,re.M))
    decode_judge_ledger(disk,directory,environment['TYPESAFEAI_API_KEY'],options.live,
        required=judge_dispatched and passed)
if not passed and melee is not None and 'native-combat: failure ' in output:
    capture_account(lambda snapshot: melee_failure_saved(output, melee, snapshot),
                    ['varp.com_mode'])
if editing:
    assert passed, 'native session failed; disk, serial log and judge records retained'
    if options.optimize:
        activated='native-opt: policy committed after text judgment and measured replay' in output
        if activated:
            assert re.search(r'^native-opt: live uses [1-9][0-9]*$',output,re.M)
        else:
            assert melee is not None and 'native-opt: continued with fixed policy after exhausted retries' in output
            assert 'native-opt: live uses 0' in output
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
if options.transfer_from:
    assert 'native-transfer: recovered {version 2 ' in output
    assert 'native-transfer: tape replay {bytes ' in output
    assert 'native-transfer: score {correct #t ' in output
    assert re.search(r'native-transfer: retained policy uses [1-9][0-9]*',output)
    assert judge_frames(disk)==donor_frames,'transferred judge journal changed'
if options.combat:
    assert re.search(r'native-combat: \{test rat-combat passed #t .*target-removed #t\}',output)
    assert output.count('operation open ') == output.count('operation close ') == 2
    expected_xp=combat_xp(output)
    if not editing:
        assert 'inference-bridge: started job' not in output, 'combat without editing dispatched inference'
        policy_label = 'retained policy' if options.transfer_from else 'fixed scripted policy'
        print(f'External model calls: 0; external judge calls: 0; model cost: $0 ({policy_label})',flush=True)
if options.craft:
    assert 'native-craft: {test adamant-platebody passed #t ' in output
    assert output.count('operation open ') == output.count('operation close ') == 2
if melee is not None:
    if not editing:
        assert 'inference-bridge: started job' not in output
    assert output.count('operation open ') == output.count('operation close ') == 2
    expected_xp, melee_levels=melee_xp(output,melee)
if options.economy:
    sale=re.search(r'^native-economy: \{test first-sale passed #t .*bronze-after-sale 0 coins-before 0 coins-after ([1-9]\d*)\}$',output,re.M)
    assert sale and 'native-economy: score {correct #t ' in output
    assert f'cost-nanousd 0 coins {sale[1]} coins-per-dollar nil' in output
    assert output.count('operation open ') == output.count('operation close ') == 2
    assert 'inference-bridge: started job' not in output
    economy_lines={f'inv=inv.inv item=obj.coins count={sale[1]}','inv=inv.inv item=obj.bronze_pickaxe count=1',
                   'stat=13 base=1 visible=1 fine_xp=62','stat=14 base=1 visible=1 fine_xp=350'}
oracle_varps=['varp.com_mode'] if melee is not None else []
if options.quest:
    ingredient_source = 'checkpoint' if options.quest_resume else 'gathered'
    assert f'native-quest: {{test cooks-assistant passed #t choices 2 quest-points 1 cooking-xp 300 ingredients-after (0 0 0) source {ingredient_source}}}' in output
    if not options.quest_resume:
        assert 'native-quest: score {correct #t ' in output
    assert output.count('operation open ') == output.count('operation close ') == 2
    assert 'inference-bridge: started job' not in output
    oracle_varps=['varp.cookquest','varp.qp','varp.cooks_assistant_ingredients']
    def quest_saved(snapshot):
        varps={key:int(value) for key,value in re.findall(r'^(varp\.\w+)=(-?\d+)$',snapshot,re.M)}
        return (varps.get('varp.cookquest',0) % 4 == 2 and varps.get('varp.qp') == 1
                and varps.get('varp.cooks_assistant_ingredients',0) % 8 == 7
                and 'stat=7 base=4 visible=4 fine_xp=3000' in snapshot
                and all(f'item=obj.{item} ' not in snapshot for item in ('bucket_milk','egg','pot_flour')))
if not options.challenge and not options.craft:
    def expected_outcome(snapshot):
        if combat_workload:
            return combat_xp_saved(expected_xp,snapshot,minimum=melee is not None)
        if options.economy:
            return economy_lines.issubset(snapshot.splitlines())
        if options.quest:
            return quest_saved(snapshot)
        return 'x=3223 z=3218 level=0' in snapshot
    saved=capture_account(expected_outcome,oracle_varps)
    assert 'rights= ' in saved and 'xp_rate_in_hundreds=100' in saved,saved
    if melee is not None:
        assert melee_levels_saved(melee_levels,saved),saved
    if options.economy:
        assert all(f'item=obj.{item} ' not in saved for item in ('bronze_bar','copper_ore','tin_ore'))
    print('Persisted native client:',saved.splitlines()[0],flush=True)
    if options.combat:
        print('Persisted combat XP:', '\n'.join(line for line in saved.splitlines() if line.startswith(('stat=0 ', 'stat=3 '))),flush=True)
    if melee is not None:
        print('Persisted melee XP:', '\n'.join(line for line in saved.splitlines()
              if line.startswith(('stat=0 ', 'stat=1 ', 'stat=2 ', 'stat=3 ', 'varp.com_mode='))),flush=True)
    if options.economy:
        print('Persisted economy:', '\n'.join(line for line in saved.splitlines()
              if line in economy_lines),flush=True)
    if options.quest:
        print('Persisted quest:', '\n'.join(line for line in saved.splitlines()
              if line.startswith(('stat=7 ', *oracle_varps))),flush=True)
    saved_outcome='combat XP' if combat_workload else 'sale proceeds and XP' if options.economy else 'quest and reward' if options.quest else 'movement'
    print('PASS: native client '+saved_outcome+' saved with normal rights and 1x XP',flush=True)

print('Elapsed seconds:',round(time.monotonic()-run_started,3),flush=True)
