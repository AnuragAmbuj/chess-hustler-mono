export type SupportedLang = 'en' | 'hi';

export interface Translations {
  // Brand
  brandName: string;
  brandTagline: string;

  // Header & Navigation
  pieces: string;
  lightTheme: string;
  darkTheme: string;
  streak: string;
  missions: string;
  vault: string;
  friends: string;
  tourneys: string;
  ads: string;
  mod: string;
  leaderboard: string;
  puzzle: string;
  spectate: string;
  signIn: string;
  signOut: string;

  // Hero Section
  heroTitle: string;
  heroSubtitle: string;
  ratedMatch: string;
  ratedMatchSub: string;
  findMatch: string;
  friendsAndChallenges: string;
  tournaments: string;
  practiceVsBot: string;

  // Dashboard Stats
  bulletRating: string;
  bulletDesc: string;
  blitzRating: string;
  blitzDesc: string;
  rapidRating: string;
  rapidDesc: string;
  totalGold: string;
  goldPerWin: string;

  // Game UI
  yourTurn: string;
  opponentTurn: string;
  resign: string;
  offerDraw: string;
  acceptDraw: string;
  declineDraw: string;
  abort: string;
  claimTimeout: string;
  check: string;
  checkmate: string;
  stalemate: string;
  drawAgreement: string;
  whiteWon: string;
  blackWon: string;
  gameDrawn: string;
  playAgain: string;
  returnToLobby: string;

  // Modals & Panels
  leaderboardTitle: string;
  rank: string;
  player: string;
  rating: string;
  peak: string;
  games: string;
  winRate: string;
  dailyPuzzleTitle: string;
  solvePuzzle: string;
  puzzleSuccess: string;
  puzzleHint: string;
  botDifficultyTitle: string;
  noviceBot: string;
  clubBot: string;
  masterBot: string;
  startPractice: string;
  close: string;
}

export const translations: Record<SupportedLang, Translations> = {
  en: {
    // Brand
    brandName: 'Chess Hustle',
    brandTagline: 'Blitz & Rapid Arena',

    // Header & Navigation
    pieces: 'Pieces',
    lightTheme: 'Light',
    darkTheme: 'Dark',
    streak: 'Streak',
    missions: 'Missions',
    vault: 'Vault',
    friends: 'Friends',
    tourneys: 'Tourneys',
    ads: '+100 Gold',
    mod: 'Mod',
    leaderboard: 'Rankings',
    puzzle: 'Puzzle',
    spectate: 'Live',
    signIn: 'Sign In',
    signOut: 'Sign Out',

    // Hero Section
    heroTitle: 'Step up to the board',
    heroSubtitle: 'Live chess matchmaking with Glicko-2 ratings, daily gold bounties, and zero-latency WebSocket clocks.',
    ratedMatch: 'Rated match',
    ratedMatchSub: '(Affects Glicko-2)',
    findMatch: 'Find Match',
    friendsAndChallenges: '👥 Friends & Challenges',
    tournaments: '🏆 Tournaments',
    practiceVsBot: 'Practice vs Bot',

    // Dashboard Stats
    bulletRating: 'Bullet Rating',
    bulletDesc: 'Fast & instinctive (< 3m)',
    blitzRating: 'Blitz Rating',
    blitzDesc: 'Primary competitive pool',
    rapidRating: 'Rapid Rating',
    rapidDesc: 'Deep calculation (10m+)',
    totalGold: 'Total Gold',
    goldPerWin: '+25 Gold per win',

    // Game UI
    yourTurn: 'Your turn',
    opponentTurn: "Opponent's turn",
    resign: 'Resign',
    offerDraw: 'Offer Draw',
    acceptDraw: 'Accept Draw',
    declineDraw: 'Decline',
    abort: 'Abort',
    claimTimeout: 'Claim Timeout',
    check: 'Check!',
    checkmate: 'Checkmate',
    stalemate: 'Stalemate',
    drawAgreement: 'Draw by agreement',
    whiteWon: 'White wins',
    blackWon: 'Black wins',
    gameDrawn: 'Game drawn',
    playAgain: 'Play Again',
    returnToLobby: 'Return to Lobby',

    // Modals & Panels
    leaderboardTitle: 'Top Ranked Players',
    rank: 'Rank',
    player: 'Player',
    rating: 'Rating',
    peak: 'Peak',
    games: 'Games',
    winRate: 'Win Rate',
    dailyPuzzleTitle: 'Daily Tactical Puzzle',
    solvePuzzle: 'Find the winning continuation',
    puzzleSuccess: 'Tactical Puzzle Solved! +50 Gold awarded.',
    puzzleHint: 'Hint',
    botDifficultyTitle: 'Choose Bot Engine Difficulty',
    noviceBot: 'Novice Engine (1000 ELO)',
    clubBot: 'Club Grinder (1500 ELO)',
    masterBot: 'Hustle Master (2000 ELO)',
    startPractice: 'Start Match',
    close: 'Close',
  },
  hi: {
    // Brand
    brandName: 'चेस हसल',
    brandTagline: 'शतरंज का असली अखाड़ा',

    // Header & Navigation
    pieces: 'मोहरे',
    lightTheme: 'दिन',
    darkTheme: 'रात',
    streak: 'स्ट्रीक',
    missions: 'मिशन',
    vault: 'वॉल्ट',
    friends: 'मित्र',
    tourneys: 'प्रतियोगिताएं',
    ads: '+100 गोल्ड',
    mod: 'मॉड',
    leaderboard: 'रैंकिंग',
    puzzle: 'पहेली',
    spectate: 'लाइव',
    signIn: 'लॉग इन',
    signOut: 'लॉग आउट',

    // Hero Section
    heroTitle: 'बिसात पर कदम रखें',
    heroSubtitle: 'लाइव शतरंज मैचमेकिंग, ग्लिको-2 रेटिंग, दैनिक गोल्ड और शून्य-विलंबता घड़ियाँ।',
    ratedMatch: 'रेटेड मैच',
    ratedMatchSub: '(रेटिंग बदलेगी)',
    findMatch: 'मैच खोजें',
    friendsAndChallenges: '👥 मित्र और चुनौतियाँ',
    tournaments: '🏆 प्रतियोगिताएं',
    practiceVsBot: 'बॉट से अभ्यास',

    // Dashboard Stats
    bulletRating: 'बुलेट रेटिंग',
    bulletDesc: 'तेज़ व आक्रामक (< 3 मिनट)',
    blitzRating: 'ब्लिट्ज़ रेटिंग',
    blitzDesc: 'मुख्य प्रतिस्पर्धा (3-5 मिनट)',
    rapidRating: 'रैपिड रेटिंग',
    rapidDesc: 'गहरी गणना (10+ मिनट)',
    totalGold: 'कुल गोल्ड',
    goldPerWin: 'जीत पर +25 गोल्ड',

    // Game UI
    yourTurn: 'आपकी बारी',
    opponentTurn: 'प्रतिद्वंद्वी की बारी',
    resign: 'इस्तीफा दें',
    offerDraw: 'ड्रा प्रस्ताव',
    acceptDraw: 'ड्रा स्वीकारें',
    declineDraw: 'अस्वीकार',
    abort: 'रद्द करें',
    claimTimeout: 'समय समाप्त दावा',
    check: 'शह!',
    checkmate: 'शह और मात!',
    stalemate: 'गतिरोध (ड्रा)',
    drawAgreement: 'परस्पर सहमति से ड्रा',
    whiteWon: 'सफेद की जीत',
    blackWon: 'काले की जीत',
    gameDrawn: 'मैच ड्रा रहा',
    playAgain: 'दोबारा खेलें',
    returnToLobby: 'लॉबी पर लौटें',

    // Modals & Panels
    leaderboardTitle: 'शीर्ष रैंकिंग खिलाड़ी',
    rank: 'स्थान',
    player: 'खिलाड़ी',
    rating: 'रेटिंग',
    peak: 'सर्वोच्च',
    games: 'खेल',
    winRate: 'जीत दर',
    dailyPuzzleTitle: 'दैनिक शतरंज पहेली',
    solvePuzzle: 'जीत की चाल खोजें',
    puzzleSuccess: 'पहेली हल हुई! +50 गोल्ड मिला।',
    puzzleHint: 'संकेत',
    botDifficultyTitle: 'बॉट इंजन कठिनाई चुनें',
    noviceBot: 'नौसिखिया इंजन (1000 ELO)',
    clubBot: 'क्लब खिलाड़ी (1500 ELO)',
    masterBot: 'हसल मास्टर (2000 ELO)',
    startPractice: 'मैच शुरू करें',
    close: 'बंद करें',
  },
};
