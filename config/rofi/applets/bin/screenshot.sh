#!/usr/bin/env bash

## Author  : Aditya Shakya (adi1090x)
## Github  : @adi1090x
#
## Applets : Screenshot
#
## Modified: JialaChang
## 		   - Ported from X11 to Hyprland/Wayland
##         - added Scroll Capture and Screen Recording

# Import Current Theme
source "$HOME"/.config/rofi/applets/shared/theme.bash
theme="$type/$style"

# Theme Elements
prompt='Screenshot'
dir="$HOME/Pictures/Screenshot"
mesg="DIR: $dir"

if [[ "$theme" == *'type-1'* ]]; then
	list_col='1'
	list_row='6'
	win_width='400px'
elif [[ "$theme" == *'type-3'* ]]; then
	list_col='1'
	list_row='6'
	win_width='120px'
elif [[ "$theme" == *'type-5'* ]]; then
	list_col='1'
	list_row='6'
	win_width='520px'
elif [[ ( "$theme" == *'type-2'* ) || ( "$theme" == *'type-4'* ) ]]; then
	list_col='6'
	list_row='1'
	win_width='670px'
fi

# Toggle state, checked up front so the menu labels below can reflect it.
scrollcap_dir="/tmp/rofi-scrollcap-$UID"
scrollcap_pid="$scrollcap_dir/pid"
scrollcap_video="$scrollcap_dir/capture.mp4"
scrollstitch_bin="$(dirname "$(readlink -f "$0")")/scrollstitch/scrollstitch"
scrollcap_active=false
if [[ -f "$scrollcap_pid" ]] && kill -0 "$(cat "$scrollcap_pid")" 2>/dev/null; then
	scrollcap_active=true
fi

screenrec_dir="/tmp/rofi-screenrec-$UID"
screenrec_pid="$screenrec_dir/pid"
screenrec_video="$screenrec_dir/capture.mp4"
screenrec_out_dir="$HOME/Videos/Screenrecord"
screenrec_active=false
if [[ -f "$screenrec_pid" ]] && kill -0 "$(cat "$screenrec_pid")" 2>/dev/null; then
	screenrec_active=true
fi

# Options
layout=`cat ${theme} | grep 'USE_ICON' | cut -d'=' -f2`
if [[ "$layout" == 'NO' ]]; then
	option_1="󰹑 Capture Desktop"
	option_2=" Capture Window"
	option_3="󰩬 Capture Area"
	option_4="󰔛 Capture in 5s"
	if $scrollcap_active; then
		option_5="⏹ Stop Scroll Capture"
	else
		option_5=" Scroll Capture"
	fi
	if $screenrec_active; then
		option_6="⏹ Stop Screen Recording"
	else
		option_6=" Screen Recording"
	fi
else
	option_1="󰹑"
	option_2=""
	option_3="󰩬"
	option_4="󰔛"
	if $scrollcap_active; then
		option_5="⏹"
	else
		option_5=""
	fi
	if $screenrec_active; then
		option_6="⏹"
	else
		option_6=""
	fi
fi

# Pre-select whichever toggle is currently recording, so the menu opens
# straight on its stop option instead of row 0.
selected_row=0
if $screenrec_active; then
	selected_row=5
elif $scrollcap_active; then
	selected_row=4
fi

# Rofi CMD
rofi_cmd() {
	rofi -theme-str "window {width: $win_width;}" \
		-theme-str "listview {columns: $list_col; lines: $list_row;}" \
		-theme-str 'textbox-prompt-colon {str: "";}' \
		-dmenu \
		-selected-row "$selected_row" \
		-p "$prompt" \
		-mesg "$mesg" \
		-markup-rows \
		-theme ${theme}
}

# Pass variables to rofi dmenu
run_rofi() {
	echo -e "$option_1\n$option_2\n$option_3\n$option_4\n$option_5\n$option_6" | rofi_cmd
}

# Screenshot
time=`date +%Y-%m-%d_%H.%M.%S`
file="${time}.png"

if [[ ! -d "$dir" ]]; then
	mkdir -p "$dir"
fi

# notify only, no viewer popup
notify_only() {
	notify_cmd_shot='dunstify -u low --replace=699'
	# ${notify_cmd_shot} "Copied to clipboard."
	if [[ -e "$dir/$file" ]]; then
		${notify_cmd_shot} "Screenshot Saved."
	else
		${notify_cmd_shot} "Screenshot Deleted."
	fi
}

# notify and open the screenshot in a viewer
notify_view() {
	notify_cmd_shot='dunstify -u low --replace=699'
	# ${notify_cmd_shot} "Copied to clipboard."
	imv "$dir/$file"
	if [[ -e "$dir/$file" ]]; then
		${notify_cmd_shot} "Screenshot Saved."
	else
		${notify_cmd_shot} "Screenshot Deleted."
	fi
}

# Copy screenshot to clipboard
copy_shot () {
	tee "$file" | wl-copy --type image/png
}

# countdown
countdown () {
	for sec in `seq $1 -1 1`; do
		dunstify -t 1000 --replace=699 "Taking shot in : $sec"
		sleep 1
	done
}

# take shots
shotnow () {
	cd ${dir} && sleep 0.5 && grim - | copy_shot
	notify_only
}

shot5 () {
	countdown '5'
	sleep 1 && cd ${dir} && grim - | copy_shot
	notify_view
}

shotwin () {
	cd ${dir} && \
	geom=$(hyprctl activewindow -j | jq -r '"\(.at[0]),\(.at[1]) \(.size[0])x\(.size[1])"') && \
	grim -g "$geom" - | copy_shot
	notify_only
}

shotarea () {
	geom=$(slurp -d)
	if [[ -z "$geom" ]]; then
		exit 0
	fi
	cd ${dir} && grim -g "$geom" - | copy_shot
	notify_only
}

# Scroll capture: press once to start recording a chosen region, press again
# to stop and stitch the scrolled frames into a single tall screenshot.
scrollcap () {
	mkdir -p "$scrollcap_dir"
	if $scrollcap_active; then
		# Already recording: stop it and stitch the result
		kill -INT "$(cat "$scrollcap_pid")"
		while kill -0 "$(cat "$scrollcap_pid")" 2>/dev/null; do
			sleep 0.2
		done
		rm -f "$scrollcap_pid"

		if [[ ! -x "$scrollstitch_bin" ]]; then
			dunstify -u low --replace=700 "Building scrollstitch..."
			(cd "$(dirname "$scrollstitch_bin")" && go build -o scrollstitch .)
		fi

		dunstify -u low --replace=700 "Stitching scroll capture..."

		read -r vw vh < <(ffprobe -v error -select_streams v:0 -show_entries stream=width,height -of csv=s=x:p=0 "$scrollcap_video" | tr 'x' ' ')
		

		cd ${dir} && ffmpeg -y -loglevel error -i "$scrollcap_video" -vf fps=8 -pix_fmt rgba -f rawvideo - \
			| "$scrollstitch_bin" "$vw" "$vh" "$file" \
			&& wl-copy --type image/png < "$dir/$file"
		
		rm -rf "$scrollcap_dir"

		if [[ -e "$dir/$file" ]]; then
			dunstify -u low --replace=700 "Scroll capture saved and copied to clipboard."
			imv "$dir/$file"
		else
			dunstify -u low --replace=700 "Scroll capture failed."
		fi
	else
		geom=$(slurp -d)
		if [[ -z "$geom" ]]; then
			exit 0
		fi
		wf-recorder -g "$geom" -f "$scrollcap_video" &>/dev/null &
		dunstify "Recording screen..."
		echo $! > "$scrollcap_pid"
	fi
}

# Screen recording: press once to start recording a chosen region to video,
# press again to stop and save it.
screenrec () {
	mkdir -p "$screenrec_dir"
	if $screenrec_active; then
		kill -INT "$(cat "$screenrec_pid")"
		while kill -0 "$(cat "$screenrec_pid")" 2>/dev/null; do
			sleep 0.2
		done
		rm -f "$screenrec_pid"

		mkdir -p "$screenrec_out_dir"
		out="$screenrec_out_dir/${time}.mp4"
		mv "$screenrec_video" "$out"

		if [[ -e "$out" ]]; then
			dunstify -u low --replace=701 "Screen recording saved to $screenrec_out_dir."
			mpv "$out"
		else
			dunstify -u low --replace=701 "Screen recording failed."
		fi
	else
		geom=$(slurp -d)
		if [[ -z "$geom" ]]; then
			exit 0
		fi
		wf-recorder -g "$geom" -f "$screenrec_video" &>/dev/null &
		dunstify "Recording screen..."
		echo $! > "$screenrec_pid"
	fi
}

# Execute Command
run_cmd() {
	if [[ "$1" == '--opt1' ]]; then
		shotnow
	elif [[ "$1" == '--opt2' ]]; then
		shotwin
	elif [[ "$1" == '--opt3' ]]; then
		shotarea
	elif [[ "$1" == '--opt4' ]]; then
		shot5
	elif [[ "$1" == '--opt5' ]]; then
		scrollcap
	elif [[ "$1" == '--opt6' ]]; then
		screenrec
	fi
}

# Actions
# Called with a --optN flag (e.g. from a keybind): run that capture directly,
# skipping the rofi menu.
if [[ -n "$1" ]]; then
	run_cmd "$1"
	exit 0
fi

chosen="$(run_rofi)"
case ${chosen} in
    $option_1)
		run_cmd --opt1
        ;;
    $option_2)
		run_cmd --opt2
        ;;
    $option_3)
		run_cmd --opt3
        ;;
    $option_4)
		run_cmd --opt4
        ;;
    $option_5)
		run_cmd --opt5
        ;;
    $option_6)
		run_cmd --opt6
        ;;
esac
