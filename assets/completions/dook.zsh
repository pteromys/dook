#compdef dook

_dook () {
	# We tell zsh about possible completions by calling various _functions
	# in zshcompsys(1)---chiefly _arguments, which sets a bunch of magic vars
	# but can't know whether they're local to the caller or not.
	# So start with some defensive local declarations I probably don't need
	# but are here anyway in case I mess with this file later.

	# _arguments will write this if we pass it -C
	local curcontext="$curcontext"
	# _arguments will write these if we pass it a `->` action
	# ref: search for "->string" in man 1 zshcompsys
	local context state state_descr line
	typeset -A opt_args

	# We just give the options explicitly since automatic parsing is brittle.
	_arguments -s -S \
		{'(--config)-c+','(-c)--config='}'[Config file path]:config file (.yml):_files' \
		'--stdin[Read fron standard input instead of searching current directory]' \
		'--color=[When to output color]:output color?:((auto\:"(default) if output seems to be a color console" never\:no always\:yes))' \
		'--paging=[When to start a pager]:start a pager?:((auto\:"(default) if output is directly to console" never\:no always\:yes))' \
		'--wrap=[When to wrap long lines]:wrap long lines?:(auto never character)' \
		'--download=[Whether to download parsers if needed]:what to do if we need to download a parser?:((ask\:"(default) ask for confirmation (disables paging)" no\:"skip the file" yes\:"download without asking"))' \
		{'(--chop-long-lines)-S','(-S)--chop-long-lines'}'[Alias for --wrap=never]' \
		'--offline[Alias for --download=no]' \
		'*'{'-p','--plain'}'[Apply no styling; specify twice to disable paging]' \
		+ '(recursion)' \
		{'-r','--recurse'}"[Recurse if the there's exactly one way to do so]" \
		"--no-recurse[Don't recurse (default)]" \
		+ '(special_action)' \
		'--dump=[Dump syntax tree of a file]:file to dump:_files' \
		'--only-names[Print only names matching the pattern]' \
		'--verbose[Print progress messages]' \
		{'-h','--help'}'[Print help]' \
		'*: :{_dook_symbols}'
}

# The last line above calls this function to generate completions for args not
# matching any of the options---here, asking dook to trawl for all names.
# This is its own function because escaping in the above {} is too fiddly.
_dook_symbols () {
	typeset -a symbols
	# only search if the user has typed anything to narrow the results
	if [ "X$PREFIX$SUFFIX" '!=' "X" ]; then
		# this is probably wrong if TAB is bound to expand-or-complete-prefix
		# and the word under the cursor is quoted
		# so hopefully everybody stayed on the default expand-or-complete
		symbols=( $(dook --only-names "${PREFIX}.*${SUFFIX}" 2>/dev/null) )
	fi
	# Send contents of `symbols` to zsh completion.
	# The description here overrides the one at the end of the _arguments call.
	_describe 'name to define' symbols
}

_dook "$@"
