import { additional, withAdditional } from './additional'

export default defineI18nLocale(() =>
  withAdditional(
    {
      meta: {
        description:
          'The Internet Ranking service for submitting BMZ Player scores and viewing rankings.',
      },
      common: {
        language: 'Language',
        openMenu: 'Open menu',
        licenses: 'Licenses',
        backHome: 'Back to home',
        loading: 'Loading...',
        search: 'Search',
        untitled: '(Untitled)',
        none: 'None',
        notRecorded: 'Not recorded',
        revoke: 'Revoke',
        save: 'Save',
      },
      nav: {
        charts: 'Charts',
        courses: 'Courses',
        players: 'Players',
        daily: "Today's results",
        profile: 'Profile',
        settings: 'Account settings',
        logout: 'Log out',
        login: 'Log in',
        register: 'Register',
      },
      home: {
        title: 'BMZ IR',
        description: 'Manage the account used to submit BMZ Player scores and view rankings.',
        loggedInAs: 'Logged in as {name}.',
        editProfile: 'Edit profile',
        chartsRanking: 'Charts and rankings',
        coursesDan: 'Courses and ranks',
        myScores: 'My scores',
      },
      auth: {
        email: 'Email address',
        password: 'Password',
        currentPassword: 'Current password',
        newPassword: 'New password',
        passwordMin: 'At least 8 characters',
        displayName: 'Display name',
        login: 'Log in',
      },
      validation: {
        emailRequired: 'Enter your email address.',
        passwordRequired: 'Enter your password.',
        currentPasswordRequired: 'Enter your current password.',
        passwordMin: 'Password must be at least 8 characters.',
        displayNameRequired: 'Enter a display name.',
      },
      login: {
        description: 'Log in to the account used to submit BMZ IR scores and view rankings.',
        noAccount: "Don't have an account?",
        forgotPassword: 'Forgot your password?',
        reset: 'Reset it',
      },
      register: {
        title: 'Create account',
        description: 'Create an account to submit scores to BMZ IR.',
        submit: 'Create account',
        haveAccount: 'Already have an account?',
      },
      logout: {
        description: 'Log out from {email}.',
        currentUser: 'the current user',
        notLoggedIn: 'You are not currently logged in.',
      },
      reset: {
        title: 'Forgot password',
        changePassword: 'Change password',
        loggedInDescription:
          'Changing your password while logged in requires your current password.',
        goSettings: 'Go to account settings',
        description: 'Send a reset link to your registered email address.',
        submit: 'Send reset email',
        remembered: 'Remembered your password?',
        unsupported:
          'Password reset by email is not supported yet. Change it from Account settings while logged in.',
      },
      licenses: {
        title: 'Web Dependency Licenses',
        description:
          'Third-party notices for web dependencies included in the Cloudflare Worker bundle.',
        openTxt: 'Open txt',
        loadFailed: 'Could not load web-dependency-licenses.txt{detail}.',
        packageCount: '{count} packages',
      },
      errors: {
        loginFailed: 'Login failed.',
        registerFailed: 'Account registration failed.',
        logoutFailed: 'Logout failed.',
      },
      apiErrors: {
        authenticationRequired: 'You need to log in.',
        invalidCredentials: 'The email address or password is incorrect.',
        invalidCurrentPassword: 'The current password is incorrect.',
        accountAlreadyExists: 'This account already exists.',
        emailAlreadyRegistered: 'This email address is already registered.',
        profileNotFound: 'Profile not found.',
        playerNotFound: 'Player not found.',
        chartNotFound: 'Chart not found.',
        courseNotFound: 'Course not found.',
        scoreNotFound: 'Score not found.',
        replayNotAvailable: 'Replay is not available.',
        sessionNotFound: 'Session not found.',
        deviceKeyNotFound: 'The signing key was not found or has already been revoked.',
      },
    },
    additional.en,
  ),
)
