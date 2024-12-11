let one;
const two = 2;

function three(x) {
	return x * x
}

function four(x) {
	this.x = x
}

four.prototype = {
	f: function flop(y) {
		return this.x * y
	},
	"eff": (y) => this.f(1 + y),
}

four.protontripe = 5

four.prototype.g = function (y) {
	return this.x + y
}

class five {
	six = 6;

	get seven() { return this.six + 1; }
}

const arr = (eight, nine = 2, ...ten) => null;
