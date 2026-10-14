// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright 2026 Niklas Krieger
// Go reference oracle for the orthoflow parity tests. Not part of
// terrastruct/d2: scripts/gen-goref.sh copies it into a d2 checkout
// (it uses TALA's internal packages, Copyright 2022 Terrastruct Inc.).

// Command basegen writes bit-exact parity fixtures for gocompat (rand.txt,
// sort.txt, math.txt) into the directory given as its argument.
package main

import (
	"bufio"
	"fmt"
	"math"
	"math/rand"
	"os"
	"slices"
	"sort"
	"strings"
)

func must(err error) {
	if err != nil {
		panic(err)
	}
}

func hexf(f float64) string { return fmt.Sprintf("%016x", math.Float64bits(f)) }

// ---------------------------------------------------------------- rand

func genRand(w *bufio.Writer) {
	seeds := []int64{0, 1, 2, 3, -1, -2, -3, 42, 991, 7, 100, 2147483647, 2147483646, 2147483648, -2147483647, -2147483648, -2147483649,
		1 << 40, math.MaxInt64, math.MinInt64, math.MinInt64 + 1, 89482311, 1234567890123, -987654321987}
	n63 := []int64{1, 2, 3, 5, 7, 10, 100, 1000, 1 << 20, 1<<31 - 1, 1 << 31, 1<<31 + 1, 1<<40 + 3, 1 << 62, 1<<62 + 1, math.MaxInt64, 3074457345618258602, 6148914691236517206}
	n31 := []int32{1, 2, 3, 7, 100, 1 << 30, 1<<30 + 1, math.MaxInt32, 1431655766, 1431655765}
	nn := []int{1, 2, 3, 5, 10, 1000, 1<<31 - 1, 1 << 31, 1<<31 + 7, 1 << 50, math.MaxInt64}
	for _, seed := range seeds {
		r := rand.New(rand.NewSource(seed))
		fmt.Fprintf(w, "seed %d\n", seed)
		for i := 0; i < 20; i++ {
			fmt.Fprintf(w, "Int63 %d\n", r.Int63())
		}
		for i := 0; i < 20; i++ {
			fmt.Fprintf(w, "Uint32 %d\n", r.Uint32())
		}
		for i := 0; i < 10; i++ {
			fmt.Fprintf(w, "Uint64 %d\n", r.Uint64())
		}
		for i := 0; i < 10; i++ {
			fmt.Fprintf(w, "Int31 %d\n", r.Int31())
		}
		for i := 0; i < 5; i++ {
			fmt.Fprintf(w, "Int %d\n", r.Int())
		}
		for _, n := range n63 {
			for i := 0; i < 3; i++ {
				fmt.Fprintf(w, "Int63n %d %d\n", n, r.Int63n(n))
			}
		}
		for _, n := range n31 {
			for i := 0; i < 3; i++ {
				fmt.Fprintf(w, "Int31n %d %d\n", n, r.Int31n(n))
			}
		}
		for _, n := range nn {
			for i := 0; i < 3; i++ {
				fmt.Fprintf(w, "Intn %d %d\n", n, r.Intn(n))
			}
		}
		for i := 0; i < 10; i++ {
			fmt.Fprintf(w, "Float64 %s\n", hexf(r.Float64()))
		}
		for _, n := range []int{0, 1, 2, 10, 37} {
			fmt.Fprintf(w, "Perm %d %s\n", n, ints(r.Perm(n)))
		}
		for _, n := range []int{0, 1, 2, 5, 50, 333} {
			v := make([]int, n)
			for i := range v {
				v[i] = i
			}
			r.Shuffle(n, func(i, j int) { v[i], v[j] = v[j], v[i] })
			fmt.Fprintf(w, "Shuffle %d %s\n", n, ints(v))
		}
		fmt.Fprintf(w, "Int63 %d\n", r.Int63())
	}
	// Long stream for one seed.
	r := rand.New(rand.NewSource(12345))
	fmt.Fprintf(w, "seed long\n")
	for i := 0; i < 5000; i++ {
		fmt.Fprintf(w, "Int63 %d\n", r.Int63())
	}
}

func ints(v []int) string {
	s := make([]string, len(v))
	for i, x := range v {
		s[i] = fmt.Sprint(x)
	}
	return "[" + strings.Join(s, ",") + "]"
}

// ---------------------------------------------------------------- sort

type item struct {
	key int
	idx int
}

type byKey []item

func (b byKey) Len() int           { return len(b) }
func (b byKey) Less(i, j int) bool { return b[i].key < b[j].key }
func (b byKey) Swap(i, j int)      { b[i], b[j] = b[j], b[i] }

func genKeys(r *rand.Rand, dist string, n int) []int {
	k := make([]int, n)
	for i := range k {
		switch dist {
		case "ties2":
			k[i] = r.Intn(2)
		case "ties3":
			k[i] = r.Intn(3)
		case "tiesHalf":
			k[i] = r.Intn(n/2 + 1)
		case "rand":
			k[i] = r.Intn(2*n + 1)
		case "sorted":
			k[i] = i / 3
		case "reversed":
			k[i] = (n - i) / 2
		case "saw":
			k[i] = i % 7
		case "allEqual":
			k[i] = 5
		case "nearlySorted":
			k[i] = i
		case "organPipe":
			if i < n/2 {
				k[i] = i
			} else {
				k[i] = n - i
			}
		}
	}
	if dist == "nearlySorted" && n > 2 {
		for s := 0; s < 3; s++ {
			a, b := r.Intn(n), r.Intn(n)
			k[a], k[b] = k[b], k[a]
		}
	}
	return k
}

func genSort(w *bufio.Writer) {
	r := rand.New(rand.NewSource(7))
	dists := []string{"ties2", "ties3", "tiesHalf", "rand", "sorted", "reversed", "saw", "allEqual", "nearlySorted", "organPipe"}
	entries := []string{"Slice", "SliceStable", "Sort", "Stable", "SortFunc", "SortStableFunc"}
	lengths := []int{}
	for n := 0; n <= 300; n++ {
		lengths = append(lengths, n)
	}
	lengths = append(lengths, 500, 1000, 2000, 5000)
	for _, n := range lengths {
		for _, dist := range dists {
			keys := genKeys(r, dist, n)
			for _, e := range entries {
				items := make([]item, n)
				for i := range items {
					items[i] = item{keys[i], i}
				}
				switch e {
				case "Slice":
					sort.Slice(items, func(i, j int) bool { return items[i].key < items[j].key })
				case "SliceStable":
					sort.SliceStable(items, func(i, j int) bool { return items[i].key < items[j].key })
				case "Sort":
					sort.Sort(byKey(items))
				case "Stable":
					sort.Stable(byKey(items))
				case "SortFunc":
					slices.SortFunc(items, func(a, b item) int { return a.key - b.key })
				case "SortStableFunc":
					slices.SortStableFunc(items, func(a, b item) int { return a.key - b.key })
				}
				perm := make([]int, n)
				for i := range items {
					perm[i] = items[i].idx
				}
				fmt.Fprintf(w, "%s %s %s %s\n", e, dist, ints(keys), ints(perm))
			}
		}
	}
	// slices.Sort / sort.Float64s on floats with NaNs (distinct payloads) and signed zeros.
	special := []float64{math.Float64frombits(0x7ff8000000000001), math.Float64frombits(0x7ff8000000000002), math.Float64frombits(0xfff8000000000003),
		0, math.Copysign(0, -1), 1, -1, math.Inf(1), math.Inf(-1), 2.5, 2.5}
	for _, n := range []int{0, 1, 2, 5, 12, 13, 30, 50, 51, 100, 257, 1000} {
		for rep := 0; rep < 4; rep++ {
			v := make([]float64, n)
			for i := range v {
				if r.Intn(3) == 0 {
					v[i] = special[r.Intn(len(special))]
				} else {
					v[i] = float64(r.Intn(10)) - 5
				}
			}
			in := fl(v)
			v2 := slices.Clone(v)
			slices.Sort(v)
			sort.Float64s(v2)
			fmt.Fprintf(w, "FloatSort %s %s %s\n", in, fl(v), fl(v2))
			iv := make([]int, n)
			for i := range iv {
				iv[i] = r.Intn(20) - 10
			}
			inI := ints(iv)
			sort.Ints(iv)
			fmt.Fprintf(w, "Ints %s %s\n", inI, ints(iv))
		}
	}
	// Search / BinarySearch
	for _, n := range []int{0, 1, 2, 7, 100} {
		v := make([]int, n)
		for i := range v {
			v[i] = r.Intn(10)
		}
		sort.Ints(v)
		for t := -1; t <= 11; t++ {
			s := sort.Search(n, func(i int) bool { return v[i] >= t })
			b, found := slices.BinarySearch(v, t)
			bf, foundf := slices.BinarySearchFunc(v, t, func(a, b int) int { return a - b })
			fmt.Fprintf(w, "Search %s %d %d %d %v %d %v\n", ints(v), t, s, b, found, bf, foundf)
		}
	}
}

func fl(v []float64) string {
	s := make([]string, len(v))
	for i, x := range v {
		s[i] = hexf(x)
	}
	return "[" + strings.Join(s, ",") + "]"
}

// ---------------------------------------------------------------- math

func genMath(w *bufio.Writer) {
	r := rand.New(rand.NewSource(99))
	specials := []float64{0, math.Copysign(0, -1), 1, -1, 0.5, -0.5, 2, -2, 3, 10, 100, 1e-300, -1e-300, 5e-324, -5e-324, 2.2250738585072014e-308,
		1e300, -1e300, math.MaxFloat64, -math.MaxFloat64, math.Inf(1), math.Inf(-1), math.NaN(), math.Pi, -math.Pi, math.Pi / 2, math.Pi / 4, math.E,
		0.7071067811865476, 1.4142135623730951, 0.7071067811865475, 0.7071067811865477, 709.782712893384, 709.7827128933841, 709.78, -745.1332191019411,
		-745.1332191019412, -708.4, -720, -740, -744.4, 1 << 29, 1<<29 + 0.5, 1e10, 1e16, 1e17, 4.5, -4.5, 2.5, -2.5, 0.49999999999999994, 1e-10,
		1.5, 3.5, 1e22, 6.02e23, 0.1, 0.2, 0.3, 1.0 / 3, 2.0 / 3, 45, 90, 180, 360, 1e-5, 4503599627370496.5, 4503599627370497, 9007199254740993}
	vals := slices.Clone(specials)
	for i := 0; i < 3000; i++ {
		switch i % 6 {
		case 0:
			vals = append(vals, r.Float64()*20-10)
		case 1:
			vals = append(vals, (r.Float64()*2-1)*1500)
		case 2:
			vals = append(vals, math.Float64frombits(r.Uint64()))
		case 3:
			vals = append(vals, r.Float64()*2)
		case 4:
			vals = append(vals, math.Ldexp(r.Float64()+0.5, r.Intn(2100)-1075))
		case 5:
			vals = append(vals, float64(r.Intn(4000)-2000)/8)
		}
	}
	un := map[string]func(float64) float64{
		"Exp": math.Exp, "Log": math.Log, "Atan": math.Atan, "Asin": math.Asin, "Acos": math.Acos, "Sin": math.Sin, "Cos": math.Cos,
		"Round": math.Round, "RoundToEven": math.RoundToEven, "Floor": math.Floor, "Ceil": math.Ceil, "Trunc": math.Trunc, "Sqrt": math.Sqrt, "Abs": math.Abs,
	}
	names := []string{"Exp", "Log", "Atan", "Asin", "Acos", "Sin", "Cos", "Round", "RoundToEven", "Floor", "Ceil", "Trunc", "Sqrt", "Abs"}
	for _, name := range names {
		f := un[name]
		for _, x := range vals {
			fmt.Fprintf(w, "%s %s %s\n", name, hexf(x), hexf(f(x)))
		}
	}
	// Exp/Log dense sweeps
	for i := -800000; i <= 800000; i += 37 {
		x := float64(i) / 1000
		fmt.Fprintf(w, "Exp %s %s\n", hexf(x), hexf(math.Exp(x)))
	}
	for i := 0; i < 20000; i++ {
		x := math.Ldexp(1+r.Float64(), r.Intn(2098)-1074)
		fmt.Fprintf(w, "Log %s %s\n", hexf(x), hexf(math.Log(x)))
		y := r.Float64()*1400 - 700
		fmt.Fprintf(w, "Exp %s %s\n", hexf(y), hexf(math.Exp(y)))
	}
	for _, x := range vals {
		fr, e := math.Frexp(x)
		fmt.Fprintf(w, "Frexp %s %s %d\n", hexf(x), hexf(fr), e)
		ip, fp := math.Modf(x)
		fmt.Fprintf(w, "Modf %s %s %s\n", hexf(x), hexf(ip), hexf(fp))
		for _, e := range []int{-1100, -1074, -1060, -1023, -60, -1, 0, 1, 7, 60, 1000, 1023, 1024, 2000} {
			fmt.Fprintf(w, "Ldexp %s %d %s\n", hexf(x), e, hexf(math.Ldexp(x, e)))
		}
	}
	bin := map[string]func(float64, float64) float64{
		"Pow": math.Pow, "Hypot": math.Hypot, "Atan2": math.Atan2, "Mod": math.Mod, "Max": math.Max, "Min": math.Min,
		"Nextafter": math.Nextafter, "Copysign": math.Copysign,
		"BuiltinMax": func(a, b float64) float64 { return max(a, b) }, "BuiltinMin": func(a, b float64) float64 { return min(a, b) },
	}
	bnames := []string{"Pow", "Hypot", "Atan2", "Mod", "Max", "Min", "Nextafter", "Copysign", "BuiltinMax", "BuiltinMin"}
	pairs := [][2]float64{}
	for _, a := range specials {
		for _, b := range specials {
			pairs = append(pairs, [2]float64{a, b})
		}
	}
	for i := 0; i < 4000; i++ {
		pairs = append(pairs, [2]float64{vals[r.Intn(len(vals))], vals[r.Intn(len(vals))]})
	}
	for _, name := range bnames {
		f := bin[name]
		for _, p := range pairs {
			fmt.Fprintf(w, "%s %s %s %s\n", name, hexf(p[0]), hexf(p[1]), hexf(f(p[0], p[1])))
		}
	}
	// Pow with fractional exponents over wide ranges
	fracs := []float64{0.5, -0.5, 1.0 / 3, 0.25, 0.75, 1.5, 2.5, -1.5, 0.1, 0.9, 3.7, -2.3, 0.333, 10.5, 1e-3}
	for i := 0; i < 20000; i++ {
		x := r.Float64() * math.Pow(10, float64(r.Intn(20)-6))
		y := fracs[r.Intn(len(fracs))]
		if i%3 == 0 {
			y = r.Float64()*20 - 10
		}
		fmt.Fprintf(w, "Pow %s %s %s\n", hexf(x), hexf(y), hexf(math.Pow(x, y)))
	}
	for i := 0; i < 5000; i++ {
		a, b := r.Float64()*2000-1000, r.Float64()*2000-1000
		fmt.Fprintf(w, "Hypot %s %s %s\n", hexf(a), hexf(b), hexf(math.Hypot(a, b)))
		fmt.Fprintf(w, "Atan2 %s %s %s\n", hexf(a), hexf(b), hexf(math.Atan2(a, b)))
		fmt.Fprintf(w, "Mod %s %s %s\n", hexf(a), hexf(b/37), hexf(math.Mod(a, b/37)))
	}
	// float -> int conversion (amd64 CVTTSD2SQ)
	for _, x := range vals {
		fmt.Fprintf(w, "F64ToInt %s %d\n", hexf(x), int64(x))
	}
	// %v formatting
	for _, x := range vals {
		fmt.Fprintf(w, "FormatV %s %v\n", hexf(x), x)
	}
}

func main() {
	dir := os.Args[1]
	for name, g := range map[string]func(*bufio.Writer){"rand.txt": genRand, "sort.txt": genSort, "math.txt": genMath} {
		f, err := os.Create(dir + "/" + name)
		must(err)
		w := bufio.NewWriter(f)
		g(w)
		must(w.Flush())
		must(f.Close())
	}
}
