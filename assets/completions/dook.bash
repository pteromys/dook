_dook_module()
{
	# delegate to shared completer for variable names and redirections
	local cur prev words cword comp_args was_split
	_comp_initialize -s -- "$@" || return

	COMPREPLY=()

	# options that take arguments in the next word
	case ${prev//[\"\']} in
		'--color'|'--paging')
			COMPREPLY=( $(compgen -W "auto never always" -- "$cur") )
			return 0
			;;
		'--wrap')
			COMPREPLY=( $(compgen -W "auto never character" -- "$cur") )
			return 0
			;;
		'--download')
			COMPREPLY=( $(compgen -W "ask yes no" -- "$cur") )
			return 0
			;;
		'--dump'|'--config')
			COMPREPLY=( $(compgen -f -o filenames -- "$cur") )
			return 0
			;;
	esac

	# partial current word
	local dcur=${cur%%[\"\']}
	dcur=${dcur##[\"\']}
	case $dcur in
		# options
		'-'*)
			local dookhelp=( $(dook --help) )
			local startswithhyphen=( "${dookhelp[@]##[^-]*}" )
			local dookoptions=( "${startswithhyphen[@]%%[,.=]*}" )
			COMPREPLY=( $(compgen -W "${dookoptions[*]@Q}") )
			return 0
			;;
		# don't complete on an empty string
		'')
			return 0
			;;
	esac

	# ask dook to search for names
	# eat a level of escapes since cur is given to us quoted
	local query REPLY escape_sub
	if [[ "$cur" = \'* ]] && _comp_dequote "$cur" || _comp_dequote "${cur}'"; then
		query="${REPLY}.*"  # this is output of _comp_dequote if it succeeded
		escape_sub="s/'/'\\\\''/g"  # put inner quotes back at the end
	elif _comp_dequote "${cur}\"" || _comp_dequote "$cur"; then
		query="${REPLY}.*"
		# if unquoted or double-quoted at end, re-escape everything
		escape_sub='s/[^A-Za-z0-9,._+:@%/-]/\\&/g'
	else  # idk I guess more exotic quoting going on
		query="${cur}.*"
		escape_sub='s/[^A-Za-z0-9,._+:@%/-]/\\&/g'
	fi
	local IFS=$'\n'
	COMPREPLY=( $( compgen -W '$( dook -i --only-names "$query" 2>/dev/null |
		sed -e '"'"'s/[][\.+*?()|{}^$#&~-]/\\&/g'"'"'";${escape_sub}" )' ) )
}

complete -F _dook_module dook
