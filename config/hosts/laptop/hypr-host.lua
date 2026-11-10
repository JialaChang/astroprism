-- Laptop-specific Hyprland values.
return {
	gaps_out = 10,
	-- Extra processes to autostart on this host.
	autostart = {
		"blueman-applet",
		"~/.config/waybar/scripts/charge-limit.sh restore",
	},
}
