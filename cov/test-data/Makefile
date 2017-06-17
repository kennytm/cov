OUTDIR := $(shell mktemp -d -t cov-output.XXXXXX)

all: trivial.gcc7.gcno trivial.clang.gcno trivial.rustc.gcno

clean:
	rm -f *.gcda *.gcno

%.gcc7.gcno: src/%.c
	gcc-7 --coverage $< -o $(OUTDIR)/$*.gcc7
	$(OUTDIR)/$*.gcc7
	mv $*.gcno $*.gcc7.gcno
	mv $*.gcda $*.gcc7.gcda

%.clang.gcno: src/%.c
	clang --coverage $< -o $(OUTDIR)/$*.clang
	$(OUTDIR)/$*.clang
	mv $*.gcno $*.clang.gcno
	mv $*.gcda $*.clang.gcda

%.rustc.gcno: src/%.rs
	rustc -g -Zprofile $< -o $(OUTDIR)/$*.rustc
	$(OUTDIR)/$*.rustc
	mv $(OUTDIR)/$*.gcno $*.rustc.gcno
	mv $(OUTDIR)/$*.gcda $*.rustc.gcda
