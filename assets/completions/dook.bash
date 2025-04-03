_dook_module()
{
	COMPREPLY=()
	local cur="${COMP_WORDS[COMP_CWORD]}"
	local prev="${COMP_WORDS[COMP_CWORD-1]}"

	# support syntax like --color=always if = is in COMP_WORDBREAKS
	local prev2="${COMP_WORDS[COMP_CWORD-2]}"
	local cur_crossing_equals
	_comp_get_words -n = -c cur_crossing_equals
	if [ "X$cur_crossing_equals" = "X${prev}=" ]; then cur=''
	elif [ "X$cur_crossing_equals" = "X${prev2}=${cur}" ]; then
		cur="${cur#=}"
		prev="$prev2"
	elif [ "X$cur_crossing_equals" = "X=${cur}" ]; then cur="$cur_crossing_equals"
	fi

	# options that take arguments in the next word
	case $prev in
		'--color'|'--paging')
			COMPREPLY=( $(compgen -W "auto never always" -- "$cur") )
			return 0
			;;
		'--wrap')
			COMPREPLY=( $(compgen -W "auto never character" -- "$cur") )
			return 0
			;;
		'--dump'|'--config')
			COMPREPLY=( $(compgen -f -o filenames -- "$cur") )
			return 0
			;;
	esac

	# partial current word
	case $cur in
		# options
		'-'*)
			local dookhelp=( $(dook --help) )
			local startswithhyphen=( "${dookhelp[@]##[^-]*}" )
			local dookoptions=( "${startswithhyphen[@]%%[,.=]*}" )
			COMPREPLY=( $(compgen -W "${dookoptions[*]@Q}" -- "$cur") )
			return 0
			;;
		# don't complete on an empty string
		'')
			return 0
			;;
	esac

	# ask dook to search for names
	COMPREPLY=( $(compgen -W '$( dook --only-names "${cur}.*" 2>/dev/null )' -- "$cur" ) )
}

complete -F _dook_module dook
