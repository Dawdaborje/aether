/**
 * Where countries are, for the map chart: a centre point each, and rough continent outlines to
 * find your way by. The map places a bubble per country, so no map data has to be shipped.
 */

export interface Country {
	iso2: string;
	iso3: string;
	name: string;
	lat: number;
	lng: number;
}

// iso2 iso3 lat lng name…
const TABLE = `
AF AFG 33.9 67.7 Afghanistan
AL ALB 41.1 20.2 Albania
DZ DZA 28.0 1.7 Algeria
AO AGO -11.2 17.9 Angola
AR ARG -38.4 -63.6 Argentina
AM ARM 40.1 45.0 Armenia
AU AUS -25.3 133.8 Australia
AT AUT 47.5 14.6 Austria
AZ AZE 40.1 47.6 Azerbaijan
BS BHS 25.0 -77.4 Bahamas
BH BHR 26.0 50.6 Bahrain
BD BGD 23.7 90.4 Bangladesh
BY BLR 53.7 27.9 Belarus
BE BEL 50.5 4.5 Belgium
BZ BLZ 17.2 -88.5 Belize
BJ BEN 9.3 2.3 Benin
BT BTN 27.5 90.4 Bhutan
BO BOL -16.3 -63.6 Bolivia
BA BIH 43.9 17.7 Bosnia and Herzegovina
BW BWA -22.3 24.7 Botswana
BR BRA -14.2 -51.9 Brazil
BN BRN 4.5 114.7 Brunei
BG BGR 42.7 25.5 Bulgaria
BF BFA 12.2 -1.6 Burkina Faso
BI BDI -3.4 29.9 Burundi
KH KHM 12.6 104.9 Cambodia
CM CMR 7.4 12.4 Cameroon
CA CAN 56.1 -106.3 Canada
CF CAF 6.6 20.9 Central African Republic
TD TCD 15.5 18.7 Chad
CL CHL -35.7 -71.5 Chile
CN CHN 35.9 104.2 China
CO COL 4.6 -74.3 Colombia
CG COG -0.2 15.8 Congo
CD COD -4.0 21.8 DR Congo
CR CRI 9.7 -83.8 Costa Rica
CI CIV 7.5 -5.5 Côte d'Ivoire
HR HRV 45.1 15.2 Croatia
CU CUB 21.5 -77.8 Cuba
CY CYP 35.1 33.4 Cyprus
CZ CZE 49.8 15.5 Czechia
DK DNK 56.3 9.5 Denmark
DJ DJI 11.8 42.6 Djibouti
DO DOM 18.7 -70.2 Dominican Republic
EC ECU -1.8 -78.2 Ecuador
EG EGY 26.8 30.8 Egypt
SV SLV 13.8 -88.9 El Salvador
GQ GNQ 1.7 10.3 Equatorial Guinea
ER ERI 15.2 39.8 Eritrea
EE EST 58.6 25.0 Estonia
SZ SWZ -26.5 31.5 Eswatini
ET ETH 9.1 40.5 Ethiopia
FJ FJI -17.7 178.1 Fiji
FI FIN 61.9 25.7 Finland
FR FRA 46.2 2.2 France
GA GAB -0.8 11.6 Gabon
GM GMB 13.4 -15.3 Gambia
GE GEO 42.3 43.4 Georgia
DE DEU 51.2 10.5 Germany
GH GHA 7.9 -1.0 Ghana
GR GRC 39.1 21.8 Greece
GT GTM 15.8 -90.2 Guatemala
GN GIN 9.9 -9.7 Guinea
GW GNB 11.8 -15.2 Guinea-Bissau
GY GUY 4.9 -58.9 Guyana
HT HTI 18.9 -72.3 Haiti
HN HND 15.2 -86.2 Honduras
HK HKG 22.4 114.1 Hong Kong
HU HUN 47.2 19.5 Hungary
IS ISL 64.9 -19.0 Iceland
IN IND 20.6 79.0 India
ID IDN -0.8 113.9 Indonesia
IR IRN 32.4 53.7 Iran
IQ IRQ 33.2 43.7 Iraq
IE IRL 53.4 -8.2 Ireland
IL ISR 31.0 34.9 Israel
IT ITA 41.9 12.6 Italy
JM JAM 18.1 -77.3 Jamaica
JP JPN 36.2 138.3 Japan
JO JOR 30.6 36.2 Jordan
KZ KAZ 48.0 66.9 Kazakhstan
KE KEN 0.0 37.9 Kenya
KW KWT 29.3 47.5 Kuwait
KG KGZ 41.2 74.8 Kyrgyzstan
LA LAO 19.9 102.5 Laos
LV LVA 56.9 24.6 Latvia
LB LBN 33.9 35.9 Lebanon
LS LSO -29.6 28.2 Lesotho
LR LBR 6.4 -9.4 Liberia
LY LBY 26.3 17.2 Libya
LT LTU 55.2 23.9 Lithuania
LU LUX 49.8 6.1 Luxembourg
MG MDG -18.8 46.9 Madagascar
MW MWI -13.3 34.3 Malawi
MY MYS 4.2 102.0 Malaysia
MV MDV 3.2 73.2 Maldives
ML MLI 17.6 -4.0 Mali
MT MLT 35.9 14.4 Malta
MR MRT 21.0 -10.9 Mauritania
MU MUS -20.3 57.6 Mauritius
MX MEX 23.6 -102.6 Mexico
MD MDA 47.4 28.4 Moldova
MN MNG 46.9 103.8 Mongolia
ME MNE 42.7 19.4 Montenegro
MA MAR 31.8 -7.1 Morocco
MZ MOZ -18.7 35.5 Mozambique
MM MMR 21.9 96.0 Myanmar
NA NAM -23.0 18.5 Namibia
NP NPL 28.4 84.1 Nepal
NL NLD 52.1 5.3 Netherlands
NZ NZL -40.9 174.9 New Zealand
NI NIC 12.9 -85.2 Nicaragua
NE NER 17.6 8.1 Niger
NG NGA 9.1 8.7 Nigeria
KP PRK 40.3 127.5 North Korea
MK MKD 41.6 21.7 North Macedonia
NO NOR 60.5 8.5 Norway
OM OMN 21.5 55.9 Oman
PK PAK 30.4 69.3 Pakistan
PS PSE 31.9 35.2 Palestine
PA PAN 8.5 -80.8 Panama
PG PNG -6.3 144.0 Papua New Guinea
PY PRY -23.4 -58.4 Paraguay
PE PER -9.2 -75.0 Peru
PH PHL 12.9 121.8 Philippines
PL POL 51.9 19.1 Poland
PT PRT 39.4 -8.2 Portugal
PR PRI 18.2 -66.6 Puerto Rico
QA QAT 25.4 51.2 Qatar
RO ROU 45.9 25.0 Romania
RU RUS 61.5 105.3 Russia
RW RWA -1.9 29.9 Rwanda
SA SAU 23.9 45.1 Saudi Arabia
SN SEN 14.5 -14.5 Senegal
RS SRB 44.0 21.0 Serbia
SL SLE 8.5 -11.8 Sierra Leone
SG SGP 1.4 103.8 Singapore
SK SVK 48.7 19.7 Slovakia
SI SVN 46.2 15.0 Slovenia
SO SOM 5.2 46.2 Somalia
ZA ZAF -30.6 22.9 South Africa
KR KOR 35.9 127.8 South Korea
SS SSD 6.9 31.3 South Sudan
ES ESP 40.5 -3.7 Spain
LK LKA 7.9 80.8 Sri Lanka
SD SDN 12.9 30.2 Sudan
SR SUR 3.9 -56.0 Suriname
SE SWE 60.1 18.6 Sweden
CH CHE 46.8 8.2 Switzerland
SY SYR 34.8 39.0 Syria
TW TWN 23.7 121.0 Taiwan
TJ TJK 38.9 71.3 Tajikistan
TZ TZA -6.4 34.9 Tanzania
TH THA 15.9 101.0 Thailand
TL TLS -8.9 125.7 Timor-Leste
TG TGO 8.6 0.8 Togo
TT TTO 10.7 -61.2 Trinidad and Tobago
TN TUN 33.9 9.5 Tunisia
TR TUR 39.0 35.2 Türkiye
TM TKM 39.0 59.6 Turkmenistan
UG UGA 1.4 32.3 Uganda
UA UKR 48.4 31.2 Ukraine
AE ARE 23.4 53.8 United Arab Emirates
GB GBR 55.4 -3.4 United Kingdom
US USA 37.1 -95.7 United States
UY URY -32.5 -55.8 Uruguay
UZ UZB 41.4 64.6 Uzbekistan
VE VEN 6.4 -66.6 Venezuela
VN VNM 14.1 108.3 Vietnam
YE YEM 15.6 48.5 Yemen
ZM ZMB -13.1 27.8 Zambia
ZW ZWE -19.0 29.2 Zimbabwe
`;

export const COUNTRIES: Country[] = TABLE.trim()
	.split('\n')
	.map((line) => {
		const [iso2, iso3, lat, lng, ...name] = line.trim().split(/\s+/);
		return { iso2, iso3, lat: Number(lat), lng: Number(lng), name: name.join(' ') };
	});

const ALIASES: Record<string, string> = {
	usa: 'US',
	'united states of america': 'US',
	america: 'US',
	uk: 'GB',
	'great britain': 'GB',
	britain: 'GB',
	england: 'GB',
	scotland: 'GB',
	wales: 'GB',
	turkey: 'TR',
	'czech republic': 'CZ',
	'russian federation': 'RU',
	korea: 'KR',
	'republic of korea': 'KR',
	'korea south': 'KR',
	'south korea': 'KR',
	uae: 'AE',
	emirates: 'AE',
	'ivory coast': 'CI',
	'democratic republic of the congo': 'CD',
	'congo kinshasa': 'CD',
	'republic of the congo': 'CG',
	'congo brazzaville': 'CG',
	holland: 'NL',
	burma: 'MM',
	'viet nam': 'VN',
	swaziland: 'SZ',
	macedonia: 'MK',
	'timor leste': 'TL',
	'east timor': 'TL',
	palestinian: 'PS',
	'hong kong sar': 'HK',
	'iran islamic republic of': 'IR',
	'syrian arab republic': 'SY',
	laos: 'LA',
	'lao pdr': 'LA',
	'cote divoire': 'CI',
	bosnia: 'BA'
};

/** Lower case, no accents or punctuation: `Côte d'Ivoire` and `cote divoire` meet. */
export function normalise(text: string): string {
	return text
		.normalize('NFD')
		.replace(/[̀-ͯ]/g, '')
		.toLowerCase()
		.replace(/[^a-z0-9 ]+/g, ' ')
		.replace(/\s+/g, ' ')
		.trim();
}

const byCode = new Map<string, Country>();
const byName = new Map<string, Country>();
for (const country of COUNTRIES) {
	byCode.set(country.iso2.toLowerCase(), country);
	byCode.set(country.iso3.toLowerCase(), country);
	byName.set(normalise(country.name), country);
}
for (const [alias, iso2] of Object.entries(ALIASES)) {
	const country = byCode.get(iso2.toLowerCase());
	if (country) byName.set(normalise(alias), country);
}

/** The country a name or a two or three letter code stands for, if there is one. */
export function findCountry(text: unknown): Country | null {
	if (typeof text !== 'string') return null;
	const clean = normalise(text);
	if (!clean) return null;
	return byName.get(clean) ?? byCode.get(clean) ?? null;
}

/** Longitude −180…180 and latitude −58…84 (no Antarctica) as fractions of the map, 0…1 from the top left. */
export const LNG = [-180, 180] as const;
export const LAT = [84, -58] as const;
export function project(lat: number, lng: number): { x: number; y: number } {
	return {
		x: (lng - LNG[0]) / (LNG[1] - LNG[0]),
		y: (lat - LAT[0]) / (LAT[1] - LAT[0])
	};
}

/** Rough silhouettes of the continents as `[lng, lat]` rings: enough to read where a bubble is. */
export const LAND: [number, number][][] = [
	[
		[-168, 66],
		[-141, 70],
		[-120, 70],
		[-95, 72],
		[-80, 68],
		[-62, 60],
		[-55, 50],
		[-67, 44],
		[-76, 35],
		[-81, 25],
		[-97, 26],
		[-97, 19],
		[-88, 16],
		[-83, 9],
		[-78, 8],
		[-92, 15],
		[-106, 23],
		[-117, 32],
		[-124, 40],
		[-125, 49],
		[-140, 60],
		[-168, 66]
	],
	[
		[-73, 78],
		[-20, 82],
		[-20, 70],
		[-43, 60],
		[-55, 68],
		[-73, 78]
	],
	[
		[-78, 8],
		[-60, 10],
		[-50, 0],
		[-35, -6],
		[-40, -22],
		[-48, -28],
		[-58, -38],
		[-65, -42],
		[-68, -52],
		[-74, -50],
		[-72, -30],
		[-70, -18],
		[-81, -5],
		[-78, 8]
	],
	[
		[-10, 36],
		[-9, 43],
		[-2, 48],
		[-5, 50],
		[5, 54],
		[8, 57],
		[12, 56],
		[20, 60],
		[25, 65],
		[28, 71],
		[50, 69],
		[70, 73],
		[100, 78],
		[140, 73],
		[170, 70],
		[180, 66],
		[160, 58],
		[155, 50],
		[142, 46],
		[135, 35],
		[128, 35],
		[122, 40],
		[121, 30],
		[110, 20],
		[108, 12],
		[105, 9],
		[100, 13],
		[98, 8],
		[103, 1],
		[98, 16],
		[92, 22],
		[80, 15],
		[77, 8],
		[72, 20],
		[67, 25],
		[57, 25],
		[56, 26],
		[50, 30],
		[48, 29],
		[52, 17],
		[43, 13],
		[35, 28],
		[35, 35],
		[28, 36],
		[24, 38],
		[22, 36],
		[16, 38],
		[12, 44],
		[8, 44],
		[3, 43],
		[-5, 36],
		[-10, 36]
	],
	[
		[-17, 21],
		[-10, 35],
		[10, 37],
		[32, 31],
		[35, 28],
		[43, 12],
		[51, 12],
		[40, -3],
		[40, -15],
		[35, -25],
		[20, -35],
		[12, -17],
		[9, -1],
		[9, 4],
		[-8, 4],
		[-17, 14],
		[-17, 21]
	],
	[
		[114, -22],
		[130, -12],
		[142, -11],
		[153, -26],
		[150, -37],
		[140, -38],
		[130, -32],
		[115, -34],
		[114, -22]
	]
];
