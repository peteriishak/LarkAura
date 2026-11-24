#!/usr/bin/env bash
# Full check in isolated headless GNOME sessions. Takes about 30 minutes.
# Keep the machine awake, because suspend ends the test session:
#   systemd-inhibit --what=idle:sleep --who=LarkAura-tests --why="Display tests" \
#     --mode=block tests/gnome-shell/full-check.sh /tmp/larkaura-check
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
B="${1:?usage: full-check.sh OUTPUT_DIR}"
mkdir -p "$B"
B="$(cd "$B" && pwd)"
SINCE=$(date '+%Y-%m-%d %H:%M:%S')
cd $ROOT && CARGO_TARGET_DIR=target-dev cargo build --release -p larkaura-engine -p larkaura-studio 2>&1 | tail -1
cd $ROOT/tests/gnome-shell
export SPIKE_PRODUCT=1 SPIKE_DRIVER=engine_check.py LARKAURA_ENGINE=$ROOT/target-dev/release/larkaura-engine LARKAURA_STATS=1000 SPIKE_STUDIO=$ROOT/target-dev/release/larkaura-studio
problems(){ echo "other=$(grep -E 'Bail out|CRITICAL|assertion .* failed|JS ERROR|panicked' $1/shell.log | grep -vc 'meta_window_set_stack_position_no_sync') stack_warning=$(grep -c meta_window_set_stack_position_no_sync $1/shell.log)"; }
S=$B/f-show; SPIKE_TIMEOUT=420 SPIKE_SCENARIO=showcase ./run.sh $S >$S.out 2>&1
python3 -I - $S <<'PY'
import json,sys
r=json.load(open(sys.argv[1]+'/results.json')); p=r.get('showcase',{})
print('SHOWCASE err',r.get('error'),'auras',len(p.get('auras',{})),'input failures',[k for k,v in p.get('auras',{}).items() if not v['input_complete']],{k:p.get(k,{}).get('input_complete') for k in('desktop','behind_windows','everywhere')})
print('BEHIND WINDOWS',p.get('behind_check'))
PY
echo "  $(problems $S)"
for i in 1 2; do S=$B/f-storm-$i; SPIKE_TIMEOUT=150 SPIKE_SCENARIO=storm ./run.sh $S >$S.out 2>&1; echo "STORM $i $(problems $S)"; done
for M in 1280x720 1920x1080 3840x2160; do S=$B/f-perf-$M; SPIKE_TIMEOUT=360 SPIKE_MONITOR=$M SPIKE_SCENARIO=perf ./run.sh $S >$S.out 2>&1
python3 -I - $S $M <<'PY'
import json,sys
r=json.load(open(sys.argv[1]+'/results.json')); print('PERF',sys.argv[2],'err',r.get('error'))
for k,v in r.get('perf',{}).items(): print(f"  {k:14} engine cpu {v['cpu_engine']:5} gpu {v['gpu_busy_engine']:5} | shell cpu {v['cpu_shell']:5} gpu {v['gpu_busy_shell']:5} | gpu clock {v.get('gpu_mhz')} MHz | gpu mem {v['gpu_memory_mib'].get('drm-total-system0')} rss {v['engine_memory_mib']['rss']} {[ (s['state'],s['fps']) for s in v['stat']]} {v['engine_threads']}")
PY
done
for run in single-1: single-2: single-3: dual-1:1920x1080 dual-2:1920x1080; do n=${run%:*}; m=${run#*:}; S=$B/f-$n; env SPIKE_SCENARIO=product ${m:+SPIKE_MONITOR2=$m} ./run.sh $S >$S.out 2>&1
python3 -I - $S $n <<'PY'
import json,sys
S,n=sys.argv[1:]; r=json.load(open(S+'/results.json')); p=r.get('product',{}); pl=p.get('placement',{})
last=lambda k:(p.get(k) or [{}])[-1]
ok={'placed':pl.get('overlays')==pl.get('displays') and pl.get('focus')=='Target A' and not pl.get('in_window_list') and pl.get('dock_type'),'input':p.get('input',{}).get('complete')==3,'active':p.get('active_usage',{}).get('gpu_percent',0)>0,'still':last('while_working').get('state')=='Still','aura':last('changed_aura').get('aura')=='quiet-snow','paused':p.get('paused_usage',{}).get('gpu_percent')==0,'full':p.get('fullscreen_usage',{}).get('gpu_percent')==0,'crash':p.get('input_after_crash',{}).get('complete'),'clean':p.get('processes_left_after_disable')==0,'final':p.get('final_input',{}).get('complete')}
print(n,'PASS' if all(ok.values()) and not r.get('error') else 'FAIL',[k for k,v in ok.items() if not v],'displays',pl.get('displays'),'recovery',p.get('crash_recovery_seconds'),'err',r.get('error'))
PY
echo "  $(problems $S)"; done
for i in 1 2; do S=$B/f-panel-$i; SPIKE_TIMEOUT=100 SPIKE_SCENARIO=panel ./run.sh $S >$S.out 2>&1; python3 -I -c "
import json;r=json.load(open('$S/results.json'));p=r.get('panel',{});print('panel $i err',r.get('error'),'aura',(p.get('chose_aura') or {}).get('aura'),'studio',p.get('studio_opened'),'icon removed',p.get('icon_removed_on_disable'))"; echo "  $(problems $S)"; done
S=$B/f-displays; SPIKE_TIMEOUT=70 SPIKE_SCENARIO=displays SPIKE_MONITOR2=1920x1080 ./run.sh $S >$S.out 2>&1; python3 -I -c "
import json;r=json.load(open('$S/results.json'));print('DISPLAYS err',r.get('error'),r.get('displays'))"; echo "  $(problems $S)"
S=$B/f-soak; SPIKE_PRODUCT= SPIKE_ENGINE=$LARKAURA_ENGINE SPIKE_SCENARIO=launch-soak SPIKE_CYCLES=24 OVERLAY_KIND=engine SPIKE_TIMEOUT=330 ./run.sh $S >$S.out 2>&1; python3 -I -c "
import json;r=json.load(open('$S/results.json'));print('soak launches',len(r.get('launches',[])),'stalled',r.get('stalled'),'err',r.get('error'))"
S=$B/f-studio; SPIKE_DRIVER=studio_check.py SPIKE_MONITOR=1920x1440 STUDIO_SIZE=1200,1320 SPIKE_TIMEOUT=240 STUDIO_CLICKS='[{"name":"decorations","x":380,"y":120},{"name":"sleigh often","x":1216,"y":421},{"name":"add snowman","x":1213,"y":305},{"name":"add tree","x":1231,"y":243},{"name":"wallpaper","x":477,"y":120,"wait":3},{"name":"night","x":823,"y":246},{"name":"use rainfall at night","x":926,"y":558,"wait":30},{"name":"through the day","x":195,"y":246},{"name":"use starfield through the day","x":153,"y":873,"wait":90},{"name":"restore wallpaper","x":1103,"y":194,"wait":8},{"name":"system","x":560,"y":120}]' ./run.sh $S >$S.out 2>&1; python3 -I -c "
import json;r=json.load(open('$S/results.json'));print('STUDIO err',r.get('error'),'running',r.get('still_running'),'failed',r.get('failed'))"; echo "  $(problems $S)"
echo "engine crash dumps: $(coredumpctl --no-pager list larkaura-engine --since "$SINCE" 2>/dev/null | grep -c -E 'SIGABRT|SIGSEGV')  shell crash dumps: $(coredumpctl --no-pager list gnome-shell --since "$SINCE" 2>/dev/null | grep -c -E 'SIGABRT|SIGSEGV|SIGTRAP')"
echo ALLDONE
