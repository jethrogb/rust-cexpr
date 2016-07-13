#define Int_456 456
#define Int_0 0
#define Int_1 0b1
#define Int_2 0x2
#define Int_63 077
#define Int_123 123
#define Int_124 124u
#define Int_125 125uL
#define Int_126 126LuL
#define Int_n3 ((-3))
#define Int_16 (((1)<<4ULL))/*comment*/ 
#define Int_13 1|8^6&2<<1
#define Int_n5 -3-2

#define CharChar_65 'A'
#define CharChar_127849 '\U0001f369' // 🍩
#define CharRaw_255 U'\xff'

#define Str_unicode u"unicode"
#define Str_long L"long"
#define Str_concat u"con" L"cat"
#define Str_concat_parens ("concat" U"_parens")
#define Str_concat_identifier (Str_concat L"_identifier")

#define Float_0 0.
#define Float_1 1f
#define Float_p1 .1
#define Float_2 2.0
#define Float_1000 1e3
#define Float_2000 2e+3
#define Float_p001 1e-3
#define Float_80 10.0*(1<<3)

#define FAIL_1(x) 3
#define FAIL_2
#define FAIL_3 0b2
#define FAIL_4 3<<1f
#define FAIL_5 UNKNOWN
#define FAIL_6 "test" Str_long Int_0
