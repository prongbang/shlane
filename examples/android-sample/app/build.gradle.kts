plugins {
    id("com.android.application")
}

android {
    namespace = "com.example.counter"
    compileSdk = 35

    defaultConfig {
        applicationId = "com.example.counter"
        minSdk = 24
        targetSdk = 35
        versionCode = 1
        versionName = "1.0.0"
    }

    // No signingConfig on purpose: the release build comes out unsigned and
    // `sign_android` signs it, which is the part worth exercising.
    buildTypes {
        release {
            isMinifyEnabled = false
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    lint {
        checkReleaseBuilds = false
    }
}

dependencies {
    testImplementation("junit:junit:4.13.2")
}
