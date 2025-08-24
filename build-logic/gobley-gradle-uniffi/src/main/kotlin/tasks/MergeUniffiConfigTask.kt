/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

package gobley.gradle.uniffi.tasks

import gobley.gradle.uniffi.Config
import gobley.gradle.uniffi.dsl.CustomType
import kotlinx.serialization.encodeToString
import net.peanuuutz.tomlkt.Toml
import org.gradle.api.DefaultTask
import org.gradle.api.file.RegularFileProperty
import org.gradle.api.provider.ListProperty
import org.gradle.api.provider.MapProperty
import org.gradle.api.provider.Property
import org.gradle.api.tasks.CacheableTask
import org.gradle.api.tasks.Input
import org.gradle.api.tasks.InputFile
import org.gradle.api.tasks.InputFiles
import org.gradle.api.tasks.Optional
import org.gradle.api.tasks.OutputFile
import org.gradle.api.tasks.PathSensitive
import org.gradle.api.tasks.PathSensitivity
import org.gradle.api.tasks.TaskAction
import java.io.File

@CacheableTask
abstract class MergeUniffiConfigTask : DefaultTask() {
    @get:InputFile
    @get:Optional
    @get:PathSensitive(PathSensitivity.RELATIVE)
    abstract val originalConfig: RegularFileProperty

    @get:Input
    @get:Optional
    abstract val crateName: Property<String>

    @get:Input
    @get:Optional
    abstract val packageRoot: Property<String>

    @get:Input
    @get:Optional
    abstract val packageName: Property<String>

    @get:Input
    @get:Optional
    abstract val cdylibName: Property<String>

    @get:Input
    @get:Optional
    abstract val kotlinMultiplatform: Property<Boolean>

    @get:Input
    @get:Optional
    abstract val kotlinTargets: ListProperty<String>

    @get:Input
    @get:Optional
    abstract val generateImmutableRecords: Property<Boolean>

    @get:Input
    @get:Optional
    abstract val omitChecksums: Property<Boolean>

    @get:Input
    @get:Optional
    abstract val customTypes: MapProperty<String, CustomType>

    @get:InputFiles
    @get:PathSensitive(PathSensitivity.RELATIVE)
    @get:Optional
    abstract val externalPackageConfigs: ListProperty<File>

    @get:Input
    @get:Optional
    abstract val kotlinVersion: Property<String>

    @get:Input
    @get:Optional
    abstract val disableJavaCleaner: Property<Boolean>

    @get:Input
    @get:Optional
    abstract val useKotlinXSerialization: Property<Boolean>

    @get:Input
    @get:Optional
    abstract val usePascalCaseEnumClass: Property<Boolean>

    @get:Input
    @get:Optional
    abstract val jvmDynamicLibraryDependencies: ListProperty<String>

    @get:Input
    @get:Optional
    abstract val androidDynamicLibraryDependencies: ListProperty<String>

    @get:Input
    @get:Optional
    abstract val dynamicLibraryDependencies: ListProperty<String>

    @get:OutputFile
    abstract val outputConfig: RegularFileProperty

    @TaskAction
    fun mergeConfig() {
        val originalConfig = originalConfig.orNull?.asFile?.let(::Config)
        val originalKotlinConfig = originalConfig?.kotlinConfig
        val kotlinConfig = Config.KotlinConfig(
            packageName = originalKotlinConfig?.packageName ?: packageName.orNull,
            cdylibName = originalKotlinConfig?.cdylibName ?: cdylibName.orNull,
            kotlinMultiplatform = originalKotlinConfig?.kotlinMultiplatform
                ?: kotlinMultiplatform.orNull,
            kotlinTargets = mergeSet(
                originalKotlinConfig?.kotlinTargets,
                kotlinTargets.orNull,
            ),
            generateImmutableRecords = originalKotlinConfig?.generateImmutableRecords
                ?: generateImmutableRecords.orNull,
            omitChecksums = originalKotlinConfig?.omitChecksums
                ?: omitChecksums.orNull,
            customTypes = mergeMap(
                originalKotlinConfig?.customTypes,
                customTypes.map {
                    it.mapValues { entry ->
                        Config.CustomType(
                            imports = entry.value.imports.orNull,
                            typeName = entry.value.typeName.orNull,
                            lift = entry.value.lift.orNull,
                            lower = entry.value.lower.orNull,
                        )
                    }
                }.orNull,
            ),
            externalPackages = mergeMap(
                originalKotlinConfig?.externalPackages,
                externalPackageConfigs.orNull?.let(::retrieveExternalPackageNames),
            ),
            kotlinTargetVersion = originalKotlinConfig?.kotlinTargetVersion
                ?: kotlinVersion.orNull?.takeIf { it.isNotBlank() },
            disableJavaCleaner = originalKotlinConfig?.disableJavaCleaner
                ?: disableJavaCleaner.orNull,
            generateSerializableTypes = originalKotlinConfig?.generateSerializableTypes
                ?: useKotlinXSerialization.orNull,
            usePascalCaseEnumClass = originalKotlinConfig?.usePascalCaseEnumClass
                ?: usePascalCaseEnumClass.orNull,
            jvmDynamicLibraryDependencies = mergeSet(
                originalKotlinConfig?.jvmDynamicLibraryDependencies,
                jvmDynamicLibraryDependencies.orNull,
            ),
            androidDynamicLibraryDependencies = mergeSet(
                originalKotlinConfig?.androidDynamicLibraryDependencies,
                androidDynamicLibraryDependencies.orNull,
            ),
            dynamicLibraryDependencies = mergeSet(
                originalKotlinConfig?.dynamicLibraryDependencies,
                dynamicLibraryDependencies.orNull,
            ),
        )
        val result = (originalConfig ?: Config()).copy(
            // Properties read by the Gradle plugins
            crateName = crateName.orNull,
            packageRoot = packageRoot.orNull,
            // Properties read by the bindgen
            bindings = Config.Bindings(
                kotlinConfig = kotlinConfig,
            ),
        )
        outputConfig.get().asFile.writeText(
            toml.encodeToString(result),
            Charsets.UTF_8,
        )
    }

    private fun retrieveExternalPackageNames(
        externalPackageConfigs: List<File>
    ): Map<String, String> {
        val result = mutableMapOf<String, String>()
        for (configFile in externalPackageConfigs) {
            val config = Config(configFile)
            val crateName = config.crateName ?: continue
            val kotlinConfig = config.kotlinConfig ?: continue
            if (!result.contains(crateName)) {
                if (kotlinConfig.packageName != null) {
                    result[crateName] = kotlinConfig.packageName
                }
            }
            if (kotlinConfig.externalPackages != null) {
                for ((externalCrateName, externalPackageName) in kotlinConfig.externalPackages) {
                    if (!result.contains(externalCrateName)) {
                        result[externalCrateName] = externalPackageName
                    }
                }
            }
        }
        return result.toMap()
    }

    private fun <T> mergeMap(
        original: Map<String, T>?,
        new: Map<String, T>?,
    ): Map<String, T>? {
        if (original == null) return new
        if (new == null) return original

        val newMap = original.toMutableMap()
        for ((newKey, newValue) in new) {
            if (!newMap.contains(newKey)) {
                newMap[newKey] = newValue
            }
        }

        return newMap.toMap()
    }

    private fun mergeSet(lhs: List<String>?, rhs: List<String>?): List<String>? {
        if (lhs == null) return rhs
        if (rhs == null) return lhs
        return lhs + rhs
    }

    companion object {
        private val toml = Toml {
            ignoreUnknownKeys = true
            explicitNulls = false
        }
    }
}